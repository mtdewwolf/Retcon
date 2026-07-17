//! Shared state for the running core service.

#![allow(missing_docs)] // Phase 2 API; public documentation lands with the generated protocol.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::sync::watch;

use crate::CoreError;
use crate::dev_servers::{DevServerRuntime, DurableDevServerRuntime};
use crate::event::EventBus;
use crate::jobs::JobSupervisor;
use crate::session_rpc::SessionRegistry;
use crate::spikes::agent::AgentRegistry;
use crate::spikes::browser::BrowserHandle;
use crate::spikes::terminal::TerminalRegistry;
use crate::verification::{DurableVerificationRunner, VerificationRunner};
use retcon_browser::{BrowserService, UnavailableBrowserService};
use retcon_filesystem::FilesystemHandle;
use retcon_permissions::ApprovalEngine;
use retcon_storage::{RecoveryReport, Storage};

#[derive(Clone)]
pub struct CoreState {
    inner: Arc<Inner>,
}

struct Inner {
    started_at: Instant,
    shutdown: watch::Sender<bool>,
    events: EventBus,
    jobs: JobSupervisor,
    terminals: TerminalRegistry,
    agents: AgentRegistry,
    sessions: SessionRegistry,
    browser: BrowserHandle,
    browser_service: Arc<dyn BrowserService>,
    storage: Storage,
    filesystem: FilesystemHandle,
    recovery: RecoveryReport,
    schema_version: Option<u32>,
    permissions: ApprovalEngine,
    verification_runner: Arc<dyn VerificationRunner>,
    dev_server_runtime: Arc<dyn DevServerRuntime>,
}

impl CoreState {
    pub fn new(data_dir: &std::path::Path) -> Result<Self, CoreError> {
        let storage = Storage::open(data_dir)?;
        let verification_runner = Arc::new(DurableVerificationRunner::new(storage.clone()));
        Self::from_storage(storage, verification_runner, None, None)
    }

    pub fn new_with_verification_runner(
        data_dir: &std::path::Path,
        verification_runner: Arc<dyn VerificationRunner>,
    ) -> Result<Self, CoreError> {
        let storage = Storage::open(data_dir)?;
        Self::from_storage(storage, verification_runner, None, None)
    }

    pub fn new_with_dev_server_runtime(
        data_dir: &std::path::Path,
        dev_server_runtime: Arc<dyn DevServerRuntime>,
    ) -> Result<Self, CoreError> {
        let storage = Storage::open(data_dir)?;
        let verification_runner = Arc::new(DurableVerificationRunner::new(storage.clone()));
        Self::from_storage(storage, verification_runner, Some(dev_server_runtime), None)
    }

    pub fn new_with_browser_service(
        data_dir: &std::path::Path,
        browser_service: Arc<dyn BrowserService>,
    ) -> Result<Self, CoreError> {
        let storage = Storage::open(data_dir)?;
        let verification_runner = Arc::new(DurableVerificationRunner::new(storage.clone()));
        Self::from_storage(storage, verification_runner, None, Some(browser_service))
    }

    fn from_storage(
        storage: Storage,
        verification_runner: Arc<dyn VerificationRunner>,
        dev_server_runtime: Option<Arc<dyn DevServerRuntime>>,
        browser_service: Option<Arc<dyn BrowserService>>,
    ) -> Result<Self, CoreError> {
        let (shutdown, _) = watch::channel(false);
        let schema_version = storage.database().schema_version().ok();
        let recovery = storage.startup_recovery().clone();
        let permissions = ApprovalEngine::new(
            storage.database().clone(),
            retcon_permissions::dev_bypass_enabled(),
        );
        let events = EventBus::open(storage.database().clone())?;
        let dev_server_runtime = dev_server_runtime.unwrap_or_else(|| {
            Arc::new(DurableDevServerRuntime::new(
                storage.clone(),
                events.clone(),
            ))
        });
        if recovery.changed_state() {
            events.emit(
                "system.recovery",
                serde_json::to_value(&recovery).unwrap_or_default(),
            )?;
        }
        Ok(Self {
            inner: Arc::new(Inner {
                started_at: Instant::now(),
                shutdown,
                events,
                jobs: JobSupervisor::with_storage(storage.database().clone()),
                terminals: TerminalRegistry::default(),
                agents: AgentRegistry::default(),
                sessions: SessionRegistry::default(),
                browser: BrowserHandle::default(),
                browser_service: browser_service
                    .unwrap_or_else(|| Arc::new(UnavailableBrowserService)),
                storage,
                filesystem: FilesystemHandle::default(),
                recovery,
                schema_version,
                permissions,
                verification_runner,
                dev_server_runtime,
            }),
        })
    }

    pub fn uptime(&self) -> Duration {
        self.inner.started_at.elapsed()
    }

    pub fn request_shutdown(&self) {
        self.inner.shutdown.send_replace(true);
    }

    pub fn emit(&self, kind: &str, payload: Value) {
        if let Err(error) = self.inner.events.emit(kind, payload) {
            tracing::error!(error_id=%error.id, "failed to persist event");
        }
    }
    pub fn events(&self) -> &EventBus {
        &self.inner.events
    }
    pub fn jobs(&self) -> &JobSupervisor {
        &self.inner.jobs
    }
    pub fn terminals(&self) -> &TerminalRegistry {
        &self.inner.terminals
    }
    pub fn agents(&self) -> &AgentRegistry {
        &self.inner.agents
    }
    pub fn sessions(&self) -> &SessionRegistry {
        &self.inner.sessions
    }
    pub fn browser(&self) -> &BrowserHandle {
        &self.inner.browser
    }
    pub fn browser_service(&self) -> &dyn BrowserService {
        self.inner.browser_service.as_ref()
    }
    pub fn storage(&self) -> &Storage {
        &self.inner.storage
    }
    pub fn filesystem(&self) -> &FilesystemHandle {
        &self.inner.filesystem
    }
    pub fn recovery(&self) -> &RecoveryReport {
        &self.inner.recovery
    }
    pub fn permissions(&self) -> &ApprovalEngine {
        &self.inner.permissions
    }
    pub fn verification_runner(&self) -> &dyn VerificationRunner {
        self.inner.verification_runner.as_ref()
    }
    pub fn dev_server_runtime(&self) -> &dyn DevServerRuntime {
        self.inner.dev_server_runtime.as_ref()
    }

    pub async fn cleanup_children(&self) {
        self.inner.terminals.shutdown();
        self.inner.agents.shutdown().await;
        self.inner.sessions.shutdown().await;
        self.inner.browser.shutdown().await;
        if let Err(error) = self.inner.browser_service.shutdown().await {
            tracing::warn!(%error, "browser-service cleanup failed during shutdown");
        }
        self.inner.filesystem.shutdown();
        if let Err(error) = self.inner.dev_server_runtime.shutdown().await {
            tracing::warn!(%error, "development-server cleanup failed during shutdown");
        }
    }

    pub fn shutdown_receiver(&self) -> watch::Receiver<bool> {
        self.inner.shutdown.subscribe()
    }

    pub fn health(&self) -> Value {
        let schema_version = self.inner.schema_version;
        let status = if schema_version.is_some() {
            "healthy"
        } else {
            "degraded"
        };
        json!({
            "status": status,
            "uptime_ms": self.uptime().as_millis(),
            "storage": {
                "status": if schema_version.is_some() { "healthy" } else { "unavailable" },
                "schema_version": schema_version,
            },
            "recovery": self.inner.recovery,
        })
    }

    pub fn capabilities(&self) -> Value {
        json!({
            "lifecycle": true,
            "structured_errors": true,
            "event_replay": true,
            "job_supervisor": true,
            "durable_storage": true,
            "layout_persistence": true,
            "storage_recovery": true,
            "session_engine": true,
            "provider_framework": true,
            "filesystem": true,
            "approval_engine": true,
            "checkpoints": true,
            "task_planning": true,
            "acceptance_gates": true,
            "durable_verification": true,
            "durable_dev_servers": true,
            "durable_browser": true,
        })
    }

    pub async fn diagnostics(&self) -> Value {
        let artifact_bytes = self.inner.storage.artifacts().disk_usage_async().await.ok();
        let browser_service = self.inner.browser_service.diagnostics().ok();
        json!({
            "process_id": std::process::id(),
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "uptime_ms": self.uptime().as_millis(),
            "version": env!("CARGO_PKG_VERSION"),
            "storage": {
                "database": self.inner.storage.database().path(),
                "artifact_bytes": artifact_bytes,
                "recovery": self.inner.recovery,
            },
            "browser_service": browser_service.as_ref().map(|diagnostics| json!({
                "service_version": diagnostics.service_version,
                "protocol_version": diagnostics.protocol_version,
                "compatible": diagnostics.compatible(),
                "healthy": diagnostics.healthy,
                "features": diagnostics.features,
            })),
        })
    }
}
