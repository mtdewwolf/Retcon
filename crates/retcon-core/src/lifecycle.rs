//! Core startup, single-instance protection, discovery, and shutdown.

#![allow(missing_docs)] // Phase 2 API; public documentation lands with the generated protocol.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use fs2::FileExt;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::Discovery;
use crate::server::Server;
use crate::state::CoreState;

pub struct CoreConfig {
    pub data_dir: PathBuf,
    pub shutdown_timeout: Duration,
}

impl CoreConfig {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            shutdown_timeout: Duration::from_secs(5),
        }
    }
}

pub struct CoreRuntime {
    config: CoreConfig,
    address: SocketAddr,
    state: CoreState,
    server_task: JoinHandle<()>,
    instance_lock: InstanceLock,
}

impl CoreRuntime {
    pub async fn start(config: CoreConfig) -> Result<Self, CoreError> {
        std::fs::create_dir_all(&config.data_dir)
            .map_err(|error| CoreError::io("create core data directory", error))?;
        let instance_lock = InstanceLock::acquire(&config.data_dir.join("core.lock"))?;
        remove_stale_discovery(&config.data_dir)?;

        let state = CoreState::new(&config.data_dir)?;
        let token = Uuid::new_v4().to_string();
        let server = Server::bind(state.clone(), token.clone()).await?;
        let address = server.address()?;
        write_discovery(&config.data_dir, address, token)?;
        let server_task = tokio::spawn(server.run());
        state.emit(
            "system.ready",
            serde_json::json!({"address": address, "version": env!("CARGO_PKG_VERSION")}),
        );

        Ok(Self {
            config,
            address,
            state,
            server_task,
            instance_lock,
        })
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub async fn wait_for_shutdown_signal(&self) -> Result<(), CoreError> {
        let mut requested = self.state.shutdown_receiver();
        if *requested.borrow() {
            return Ok(());
        }
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result.map_err(|error| CoreError::io("listen for shutdown signal", error))?;
            }
            changed = requested.changed() => {
                changed.map_err(|_| CoreError::new(
                    ErrorCode::Shutdown,
                    ErrorSource::Lifecycle,
                    "The core shutdown channel closed unexpectedly.",
                    "shutdown watch sender dropped",
                ))?;
            }
        }
        Ok(())
    }

    pub async fn shutdown(mut self) -> Result<(), CoreError> {
        self.state
            .emit("system.shutdown", serde_json::json!({"reason":"requested"}));
        self.state.request_shutdown();
        self.state.jobs().shutdown();
        self.state.cleanup_children().await;
        if let Err(error) = self.state.storage().maintain() {
            tracing::warn!(%error, "database maintenance failed during shutdown");
        }
        match self
            .state
            .artifacts()
            .cleanup_referenced(self.state.storage(), Duration::from_secs(30 * 24 * 60 * 60))
        {
            Ok(report) if report.removed_files > 0 => tracing::info!(
                removed_files = report.removed_files,
                removed_bytes = report.removed_bytes,
                "expired unreferenced artifacts"
            ),
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "artifact retention cleanup failed"),
        }
        if tokio::time::timeout(self.config.shutdown_timeout, &mut self.server_task)
            .await
            .is_err()
        {
            tracing::warn!("graceful shutdown timed out; forcing server task to stop");
            self.server_task.abort();
            let _ = self.server_task.await;
        }

        remove_file_if_exists(&self.config.data_dir.join("core.json"))?;
        self.instance_lock.release()?;
        tracing::info!("Retcon core stopped cleanly");
        Ok(())
    }
}

struct InstanceLock {
    file: File,
}

impl InstanceLock {
    fn acquire(path: &Path) -> Result<Self, CoreError> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .map_err(|error| CoreError::io("open core process lock", error))?;
        file.try_lock_exclusive().map_err(|error| {
            CoreError::new(
                ErrorCode::AlreadyRunning,
                ErrorSource::Lifecycle,
                "Retcon Core is already running.",
                format!("could not acquire {}: {error}", path.display()),
            )
            .suggested_fix("Use the running Retcon instance or stop it before trying again.")
        })?;
        Ok(Self { file })
    }

    fn release(&mut self) -> Result<(), CoreError> {
        FileExt::unlock(&self.file)
            .map_err(|error| CoreError::io("release core process lock", error))
    }
}

fn remove_stale_discovery(data_dir: &Path) -> Result<(), CoreError> {
    remove_file_if_exists(&data_dir.join("core.json.tmp"))?;
    remove_file_if_exists(&data_dir.join("core.json"))
}

fn remove_file_if_exists(path: &Path) -> Result<(), CoreError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CoreError::io(&format!("remove {}", path.display()), error)),
    }
}

fn write_discovery(data_dir: &Path, address: SocketAddr, token: String) -> Result<(), CoreError> {
    let discovery = Discovery {
        address: address.to_string(),
        token,
        pid: std::process::id(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        protocol_version: 1,
    };
    let bytes = serde_json::to_vec_pretty(&discovery).map_err(|error| {
        CoreError::new(
            ErrorCode::Internal,
            ErrorSource::Lifecycle,
            "Retcon could not create its discovery information.",
            error.to_string(),
        )
    })?;
    let temporary_path = data_dir.join("core.json.tmp");
    let final_path = data_dir.join("core.json");
    let mut file = File::create(&temporary_path)
        .map_err(|error| CoreError::io("create discovery file", error))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| CoreError::io("write discovery file", error))?;
    std::fs::rename(&temporary_path, &final_path)
        .map_err(|error| CoreError::io("publish discovery file", error))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use retcon_storage::{NewProject, NewSession, NewTask};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpStream;

    #[tokio::test]
    async fn lifecycle_writes_and_removes_discovery() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("core.json"), "stale").unwrap();

        let runtime = CoreRuntime::start(CoreConfig::new(directory.path().to_owned()))
            .await
            .unwrap();
        let discovery: Discovery =
            serde_json::from_slice(&std::fs::read(directory.path().join("core.json")).unwrap())
                .unwrap();
        assert_eq!(discovery.address, runtime.address().to_string());
        assert!(directory.path().join("retcon.db").is_file());

        runtime.shutdown().await.unwrap();
        assert!(!directory.path().join("core.json").exists());
    }

    #[tokio::test]
    async fn second_core_cannot_use_the_same_data_directory() {
        let directory = tempfile::tempdir().unwrap();
        let first = CoreRuntime::start(CoreConfig::new(directory.path().to_owned()))
            .await
            .unwrap();
        let second = CoreRuntime::start(CoreConfig::new(directory.path().to_owned())).await;

        assert!(matches!(
            second,
            Err(CoreError {
                code: ErrorCode::AlreadyRunning,
                ..
            })
        ));
        first.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn health_and_shutdown_endpoints_work() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = CoreRuntime::start(CoreConfig::new(directory.path().to_owned()))
            .await
            .unwrap();
        let discovery: Discovery =
            serde_json::from_slice(&std::fs::read(directory.path().join("core.json")).unwrap())
                .unwrap();
        let stream = TcpStream::connect(runtime.address()).await.unwrap();
        let (reader, mut writer) = stream.into_split();
        writer
            .write_all(
                format!(
                    "{{\"auth\":\"{}\"}}\n{{\"id\":1,\"method\":\"core.health\"}}\n{{\"id\":2,\"method\":\"core.shutdown\"}}\n",
                    discovery.token
                )
                .as_bytes(),
            )
            .await
            .unwrap();

        let mut lines = BufReader::new(reader).lines();
        let health: serde_json::Value =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        let shutdown: serde_json::Value =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(health["result"]["status"], "healthy");
        assert_eq!(health["result"]["storage"]["status"], "healthy");
        assert_eq!(health["result"]["storage"]["schema_version"], 5);
        assert_eq!(shutdown["result"]["accepted"], true);

        runtime.wait_for_shutdown_signal().await.unwrap();
        runtime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn session_and_task_state_survive_a_core_restart() {
        let directory = tempfile::tempdir().unwrap();
        let first = CoreRuntime::start(CoreConfig::new(directory.path().to_owned()))
            .await
            .unwrap();
        let project = first
            .state
            .storage()
            .projects()
            .create(&NewProject::new("Retcon"))
            .unwrap();
        let mut new_session = NewSession::new(project.id, "Durable session");
        new_session.status = "running".into();
        let session = first
            .state
            .storage()
            .sessions()
            .create(&new_session)
            .unwrap();
        let mut new_task = NewTask::new("Durable task");
        new_task.session_id = Some(session.id);
        let task = first.state.storage().tasks().create(&new_task).unwrap();
        first.shutdown().await.unwrap();

        let second = CoreRuntime::start(CoreConfig::new(directory.path().to_owned()))
            .await
            .unwrap();
        assert_eq!(
            second
                .state
                .storage()
                .sessions()
                .get(session.id)
                .unwrap()
                .unwrap()
                .status,
            "disconnected"
        );
        assert_eq!(
            second
                .state
                .storage()
                .tasks()
                .get(task.id)
                .unwrap()
                .unwrap()
                .status,
            "pending"
        );
        assert_eq!(second.state.recovery().interrupted_sessions, 1);
        assert_eq!(second.state.recovery().active_tasks, 1);
        second.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn corrupted_storage_surfaces_an_actionable_core_error() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("retcon.db"), b"not sqlite").unwrap();
        let result = CoreRuntime::start(CoreConfig::new(directory.path().to_owned())).await;
        let error = match result {
            Ok(runtime) => {
                runtime.shutdown().await.unwrap();
                panic!("corrupt database opened")
            }
            Err(error) => error,
        };
        assert_eq!(error.source, ErrorSource::Storage);
        assert!(error.user_message.contains("damaged"));
        assert!(error.suggested_fix.as_deref().unwrap().contains("backup"));
    }
}
