//! Durable adapter for the standalone development-server runtime.

#![allow(missing_docs)]

use std::collections::HashMap;
use std::future::Future;
use std::io::Read;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use retcon_devserver::{
    DevServer, DevServerCommand, DevServerEvent as RuntimeEvent, LogStream, PortAllocator,
    ProjectCommand, StartOptions, detect_start_command,
};
use retcon_storage::{DevServerInstance, DevServerLaunchConfig, Storage};
use serde_json::{Value, json};
use tokio::sync::watch;
use uuid::Uuid;

use crate::event::EventBus;

const RUNTIME_ACTOR: &str = "dev_server_runtime";
const MAX_LOG_BYTES: usize = 512 * 1024;

pub type DevServerFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;

/// Process metadata returned after a development server starts.
#[derive(Clone, Debug)]
pub struct DevServerStarted {
    pub pid: Option<i64>,
    pub url: String,
    pub preview: Value,
}

/// Runtime boundary used by durable RPC lifecycle orchestration.
pub trait DevServerRuntime: Send + Sync {
    fn start<'a>(
        &'a self,
        config: &'a DevServerLaunchConfig,
        instance: &'a DevServerInstance,
    ) -> DevServerFuture<'a, DevServerStarted>;
    fn stop<'a>(&'a self, instance: &'a DevServerInstance) -> DevServerFuture<'a, Vec<u8>>;
    fn logs(&self, instance: &DevServerInstance) -> Result<Vec<u8>, String>;
    fn shutdown(&self) -> DevServerFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}

/// Inert runtime used by tests that explicitly do not execute processes.
#[derive(Default)]
pub struct NoopDevServerRuntime;

impl DevServerRuntime for NoopDevServerRuntime {
    fn start<'a>(
        &'a self,
        _config: &'a DevServerLaunchConfig,
        _instance: &'a DevServerInstance,
    ) -> DevServerFuture<'a, DevServerStarted> {
        Box::pin(async { Err("development server runtime is not installed".into()) })
    }

    fn stop<'a>(&'a self, _instance: &'a DevServerInstance) -> DevServerFuture<'a, Vec<u8>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn logs(&self, _instance: &DevServerInstance) -> Result<Vec<u8>, String> {
        Ok(Vec::new())
    }
}

#[derive(Clone)]
struct ActiveServer {
    server: Arc<DevServer>,
    logs: Arc<Mutex<Vec<u8>>>,
    terminal: watch::Receiver<bool>,
}

type ActiveServers = Arc<Mutex<HashMap<Uuid, ActiveServer>>>;

/// Connects standalone processes to durable instances, artifacts, and events.
pub struct DurableDevServerRuntime {
    storage: Storage,
    events: EventBus,
    allocator: PortAllocator,
    active: ActiveServers,
}

impl DurableDevServerRuntime {
    #[must_use]
    pub fn new(storage: Storage, events: EventBus) -> Self {
        Self {
            storage,
            events,
            allocator: PortAllocator::default(),
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn start_instance(
        &self,
        config: DevServerLaunchConfig,
        instance: DevServerInstance,
    ) -> Result<DevServerStarted, String> {
        let root = instance_root(&self.storage, &config)?;
        let mut command = configured_command(&root, &config)?;
        validate_local_host(&config.config.host)?;
        command.env.extend(config.environment.clone());
        command
            .env
            .insert("HOST".into(), config.config.host.clone());
        let requested_port = u16::try_from(instance.port)
            .map_err(|_| format!("invalid durable development-server port {}", instance.port))?;
        let options = StartOptions {
            project_key: instance.project_id.to_string(),
            worktree_key: instance
                .worktree_id
                .map_or_else(|| root.to_string_lossy().into_owned(), |id| id.to_string()),
            requested_port: Some(requested_port),
            allow_alternate_port: true,
            startup_timeout: Duration::from_secs(60),
            max_output_bytes: MAX_LOG_BYTES / 2,
        };
        let server = Arc::new(DevServer::new(self.allocator.clone()));
        let receiver = server.subscribe();
        let logs = Arc::new(Mutex::new(Vec::new()));
        let (terminal_sender, terminal) = watch::channel(false);
        let active = ActiveServer {
            server: server.clone(),
            logs: logs.clone(),
            terminal: terminal.clone(),
        };
        self.active
            .lock()
            .map_err(|_| "development-server runtime state is poisoned".to_owned())?
            .insert(instance.id, active);
        tokio::spawn(monitor_instance(
            instance.id,
            self.storage.clone(),
            self.events.clone(),
            self.active.clone(),
            logs,
            receiver,
            terminal_sender,
        ));

        let started = match server.start(command, options).await {
            Ok(started) => started,
            Err(error) => {
                let _ = server.stop().await;
                wait_for_terminal(terminal).await;
                return Err(error.to_string());
            }
        };
        if i64::from(started.ready.port) != instance.port
            && let Err(error) = self
                .storage
                .database()
                .dev_servers()
                .reassign_instance_port(instance.id, i64::from(started.ready.port), RUNTIME_ACTOR)
        {
            let _ = server.stop().await;
            return Err(error.to_string());
        }
        let preview = json!({
            "kind": "localhost",
            "framework": started.framework,
            "host": config.config.host,
            "port": started.ready.port,
            "url": started.ready.url,
            "worktreeId": instance.worktree_id,
            "detectedFromOutput": started.ready.detected_from_output,
        });
        Ok(DevServerStarted {
            pid: None,
            url: started.ready.url,
            preview,
        })
    }

    async fn stop_instance(&self, instance: DevServerInstance) -> Result<Vec<u8>, String> {
        let active = self
            .active
            .lock()
            .map_err(|_| "development-server runtime state is poisoned".to_owned())?
            .get(&instance.id)
            .cloned();
        if let Some(active) = active {
            active
                .server
                .stop()
                .await
                .map_err(|error| error.to_string())?;
            wait_for_terminal(active.terminal).await;
            return active
                .logs
                .lock()
                .map(|logs| logs.clone())
                .map_err(|_| "development-server log state is poisoned".into());
        }
        read_persisted_logs(&self.storage, &instance)
    }

    async fn shutdown_all(&self) -> Result<(), String> {
        let active: Vec<_> = self
            .active
            .lock()
            .map_err(|_| "development-server runtime state is poisoned".to_owned())?
            .iter()
            .map(|(id, server)| (*id, server.clone()))
            .collect();
        for (instance_id, server) in active {
            let _ = self
                .storage
                .database()
                .dev_servers()
                .begin_stop(instance_id, RUNTIME_ACTOR);
            server
                .server
                .stop()
                .await
                .map_err(|error| error.to_string())?;
            wait_for_terminal(server.terminal).await;
            let _ = self
                .storage
                .database()
                .dev_servers()
                .mark_stopped(instance_id, RUNTIME_ACTOR);
        }
        Ok(())
    }
}

impl DevServerRuntime for DurableDevServerRuntime {
    fn start<'a>(
        &'a self,
        config: &'a DevServerLaunchConfig,
        instance: &'a DevServerInstance,
    ) -> DevServerFuture<'a, DevServerStarted> {
        let config = config.clone();
        let instance = instance.clone();
        Box::pin(async move { self.start_instance(config, instance).await })
    }

    fn stop<'a>(&'a self, instance: &'a DevServerInstance) -> DevServerFuture<'a, Vec<u8>> {
        let instance = instance.clone();
        Box::pin(async move { self.stop_instance(instance).await })
    }

    fn logs(&self, instance: &DevServerInstance) -> Result<Vec<u8>, String> {
        let active = self
            .active
            .lock()
            .map_err(|_| "development-server runtime state is poisoned".to_owned())?
            .get(&instance.id)
            .cloned();
        if let Some(active) = active {
            return active
                .logs
                .lock()
                .map(|logs| logs.clone())
                .map_err(|_| "development-server log state is poisoned".into());
        }
        read_persisted_logs(&self.storage, instance)
    }

    fn shutdown(&self) -> DevServerFuture<'_, ()> {
        Box::pin(async move { self.shutdown_all().await })
    }
}

#[allow(clippy::too_many_lines)]
async fn monitor_instance(
    instance_id: Uuid,
    storage: Storage,
    events: EventBus,
    active: ActiveServers,
    logs: Arc<Mutex<Vec<u8>>>,
    mut receiver: retcon_devserver::EventStream,
    terminal: watch::Sender<bool>,
) {
    loop {
        let event = match receiver.recv().await {
            Ok(event) => event,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                let payload = json!({"skipped": skipped});
                let _ = storage.database().dev_servers().record_runtime_event(
                    instance_id,
                    "events_lagged",
                    RUNTIME_ACTOR,
                    &payload,
                );
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        };
        match event {
            RuntimeEvent::Starting {
                framework, port, ..
            } => {
                persist_runtime_event(
                    &storage,
                    &events,
                    instance_id,
                    "runtime_starting",
                    json!({"framework": framework, "port": port}),
                );
            }
            RuntimeEvent::Log {
                stream,
                text,
                retained_bytes,
                total_bytes,
                ..
            } => {
                append_log(&logs, stream, &text);
                let artifact = persist_log_snapshot(&storage, instance_id, &logs).ok();
                let payload = json!({
                    "stream": stream,
                    "retainedBytes": retained_bytes,
                    "totalBytes": total_bytes,
                    "artifactHash": artifact,
                });
                let _ = storage.database().dev_servers().record_runtime_event(
                    instance_id,
                    "log_updated",
                    RUNTIME_ACTOR,
                    &payload,
                );
                let _ = events.emit_volatile(
                    "dev_server.log",
                    json!({
                        "instanceId": instance_id,
                        "stream": stream,
                        "text": text,
                        "retainedBytes": retained_bytes,
                        "totalBytes": total_bytes,
                    }),
                );
            }
            RuntimeEvent::OutputTruncated {
                stream,
                limit_bytes,
                ..
            } => persist_runtime_event(
                &storage,
                &events,
                instance_id,
                "output_truncated",
                json!({"stream": stream, "limitBytes": limit_bytes}),
            ),
            RuntimeEvent::Ready { info, .. } => persist_runtime_event(
                &storage,
                &events,
                instance_id,
                "ready_detected",
                json!({"url": info.url, "port": info.port, "detectedFromOutput": info.detected_from_output}),
            ),
            RuntimeEvent::HotReload { message, .. } => persist_runtime_event(
                &storage,
                &events,
                instance_id,
                "hot_reload",
                json!({"message": message}),
            ),
            RuntimeEvent::StartupFailed { message, .. } => {
                persist_runtime_event(
                    &storage,
                    &events,
                    instance_id,
                    "startup_failed",
                    json!({"failure": message}),
                );
                break;
            }
            RuntimeEvent::Crashed { exit_code, .. } => {
                let _ = persist_log_snapshot(&storage, instance_id, &logs);
                let failure = format!("development server crashed with exit code {exit_code:?}");
                let _ = storage.database().dev_servers().mark_failed(
                    instance_id,
                    &failure,
                    RUNTIME_ACTOR,
                );
                let _ = events.emit(
                    "dev_server.crashed",
                    json!({"instanceId": instance_id, "exitCode": exit_code, "failure": failure}),
                );
                break;
            }
            RuntimeEvent::Exited { exit_code, .. } => {
                let _ = persist_log_snapshot(&storage, instance_id, &logs);
                let _ = storage
                    .database()
                    .dev_servers()
                    .mark_exited(instance_id, RUNTIME_ACTOR);
                let _ = events.emit(
                    "dev_server.exited",
                    json!({"instanceId": instance_id, "exitCode": exit_code}),
                );
                break;
            }
            RuntimeEvent::Stopped { .. } => {
                let _ = persist_log_snapshot(&storage, instance_id, &logs);
                persist_runtime_event(&storage, &events, instance_id, "runtime_stopped", json!({}));
                break;
            }
        }
    }
    terminal.send_replace(true);
    if let Ok(mut active) = active.lock() {
        active.remove(&instance_id);
    }
}

fn instance_root(storage: &Storage, config: &DevServerLaunchConfig) -> Result<PathBuf, String> {
    if let Some(worktree_id) = config.config.worktree_id {
        return storage
            .database()
            .git_worktrees()
            .get(worktree_id)
            .map_err(|error| error.to_string())?
            .map(|worktree| PathBuf::from(worktree.path))
            .ok_or_else(|| "configured development-server worktree was not found".into());
    }
    storage
        .database()
        .projects()
        .primary_location_path(config.config.project_id)
        .map_err(|error| error.to_string())?
        .map(PathBuf::from)
        .ok_or_else(|| "project has no repository location for development server".into())
}

fn configured_command(
    root: &std::path::Path,
    config: &DevServerLaunchConfig,
) -> Result<DevServerCommand, String> {
    detect_start_command(
        root,
        &[ProjectCommand {
            key: "dev-server-start".into(),
            kind: "custom".into(),
            command: config.config.command.clone(),
            cwd: Some(PathBuf::from(&config.config.cwd)),
            enabled: true,
        }],
    )
    .map_err(|error| error.to_string())
}

fn validate_local_host(host: &str) -> Result<(), String> {
    if matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]") {
        Ok(())
    } else {
        Err(format!(
            "development-server host '{host}' is not a localhost address"
        ))
    }
}

fn append_log(logs: &Mutex<Vec<u8>>, stream: LogStream, text: &str) {
    let Ok(mut logs) = logs.lock() else {
        return;
    };
    let prefix: &[u8] = match stream {
        LogStream::Stdout => b"[stdout] ",
        LogStream::Stderr => b"[stderr] ",
    };
    let remaining = MAX_LOG_BYTES.saturating_sub(logs.len());
    logs.extend_from_slice(&prefix[..prefix.len().min(remaining)]);
    let remaining = MAX_LOG_BYTES.saturating_sub(logs.len());
    logs.extend_from_slice(&text.as_bytes()[..text.len().min(remaining)]);
    if !text.ends_with('\n') && logs.len() < MAX_LOG_BYTES {
        logs.push(b'\n');
    }
}

fn persist_log_snapshot(
    storage: &Storage,
    instance_id: Uuid,
    logs: &Mutex<Vec<u8>>,
) -> Result<String, String> {
    let snapshot = logs
        .lock()
        .map_err(|_| "development-server log state is poisoned".to_owned())?
        .clone();
    let artifact = storage
        .artifacts()
        .store_bytes(&snapshot)
        .map_err(|error| error.to_string())?;
    storage
        .database()
        .dev_servers()
        .set_log_artifact(instance_id, &artifact.hash)
        .map_err(|error| error.to_string())?;
    Ok(artifact.hash)
}

fn persist_runtime_event(
    storage: &Storage,
    events: &EventBus,
    instance_id: Uuid,
    kind: &str,
    payload: Value,
) {
    let _ = storage.database().dev_servers().record_runtime_event(
        instance_id,
        kind,
        RUNTIME_ACTOR,
        &payload,
    );
    let _ = events.emit(
        format!("dev_server.{kind}"),
        json!({"instanceId": instance_id, "details": payload}),
    );
}

fn read_persisted_logs(storage: &Storage, instance: &DevServerInstance) -> Result<Vec<u8>, String> {
    let Some(hash) = &instance.log_artifact_hash else {
        return Ok(Vec::new());
    };
    let mut file = storage
        .artifacts()
        .get(hash)
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    bytes.truncate(MAX_LOG_BYTES);
    Ok(bytes)
}

async fn wait_for_terminal(mut terminal: watch::Receiver<bool>) {
    if *terminal.borrow() {
        return;
    }
    let _ = tokio::time::timeout(Duration::from_secs(5), terminal.changed()).await;
}
