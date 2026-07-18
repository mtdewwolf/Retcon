//! Shared state for the running core service.

#![allow(missing_docs)] // Phase 2 API; public documentation lands with the generated protocol.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::sync::watch;

use crate::CoreError;
use crate::browser_verification::{BrowserVerificationRunner, ServiceBrowserVerificationRunner};
use crate::dev_servers::{DevServerRuntime, DurableDevServerRuntime};
use crate::diagnostics::DiagnosticsService;
use crate::event::EventBus;
use crate::jobs::JobSupervisor;
use crate::session_rpc::SessionRegistry;
use crate::spikes::agent::AgentRegistry;
use crate::spikes::browser::BrowserHandle;
use crate::spikes::terminal::TerminalRegistry;
use crate::verification::{DurableVerificationRunner, VerificationRunner};
use retcon_browser::{BrowserService, NodeBrowserService};
use retcon_filesystem::FilesystemHandle;
use retcon_permissions::ApprovalEngine;
use retcon_platform::{IdeIntegration, SystemIdeIntegration};
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
    browser_verification_runner: Arc<dyn BrowserVerificationRunner>,
    storage: Storage,
    filesystem: FilesystemHandle,
    recovery: RecoveryReport,
    schema_version: Option<u32>,
    permissions: ApprovalEngine,
    verification_runner: Arc<dyn VerificationRunner>,
    dev_server_runtime: Arc<dyn DevServerRuntime>,
    diagnostics: Arc<DiagnosticsService>,
    ide: Arc<dyn IdeIntegration>,
    runtime_observability_installed: AtomicBool,
}

impl CoreState {
    pub fn new(data_dir: &std::path::Path) -> Result<Self, CoreError> {
        let storage = Storage::open(data_dir)?;
        let verification_runner = Arc::new(DurableVerificationRunner::new(storage.clone()));
        Self::from_storage(storage, verification_runner, None, None, None, None)
    }

    pub fn new_with_verification_runner(
        data_dir: &std::path::Path,
        verification_runner: Arc<dyn VerificationRunner>,
    ) -> Result<Self, CoreError> {
        let storage = Storage::open(data_dir)?;
        Self::from_storage(storage, verification_runner, None, None, None, None)
    }

    pub fn new_with_dev_server_runtime(
        data_dir: &std::path::Path,
        dev_server_runtime: Arc<dyn DevServerRuntime>,
    ) -> Result<Self, CoreError> {
        let storage = Storage::open(data_dir)?;
        let verification_runner = Arc::new(DurableVerificationRunner::new(storage.clone()));
        Self::from_storage(
            storage,
            verification_runner,
            Some(dev_server_runtime),
            None,
            None,
            None,
        )
    }

    pub fn new_with_browser_service(
        data_dir: &std::path::Path,
        browser_service: Arc<dyn BrowserService>,
    ) -> Result<Self, CoreError> {
        let storage = Storage::open(data_dir)?;
        let verification_runner = Arc::new(DurableVerificationRunner::new(storage.clone()));
        Self::from_storage(
            storage,
            verification_runner,
            None,
            Some(browser_service),
            None,
            None,
        )
    }

    pub fn new_with_browser_verification_runner(
        data_dir: &std::path::Path,
        runner: Arc<dyn BrowserVerificationRunner>,
    ) -> Result<Self, CoreError> {
        let storage = Storage::open(data_dir)?;
        let verification_runner = Arc::new(DurableVerificationRunner::new(storage.clone()));
        Self::from_storage(storage, verification_runner, None, None, Some(runner), None)
    }

    pub fn new_with_ide_integration(
        data_dir: &std::path::Path,
        ide: Arc<dyn IdeIntegration>,
    ) -> Result<Self, CoreError> {
        let storage = Storage::open(data_dir)?;
        let verification_runner = Arc::new(DurableVerificationRunner::new(storage.clone()));
        Self::from_storage(storage, verification_runner, None, None, None, Some(ide))
    }

    fn from_storage(
        storage: Storage,
        verification_runner: Arc<dyn VerificationRunner>,
        dev_server_runtime: Option<Arc<dyn DevServerRuntime>>,
        browser_service: Option<Arc<dyn BrowserService>>,
        browser_verification_runner: Option<Arc<dyn BrowserVerificationRunner>>,
        ide: Option<Arc<dyn IdeIntegration>>,
    ) -> Result<Self, CoreError> {
        let (shutdown, _) = watch::channel(false);
        let schema_version = storage.database().schema_version().ok();
        let recovery = storage.startup_recovery().clone();
        let permissions = ApprovalEngine::new(
            storage.database().clone(),
            retcon_permissions::dev_bypass_enabled(),
        );
        let events = EventBus::open(storage.database().clone())?;
        let diagnostics = DiagnosticsService::open(storage.clone())?;
        let dev_server_runtime = dev_server_runtime.unwrap_or_else(|| {
            Arc::new(DurableDevServerRuntime::new(
                storage.clone(),
                events.clone(),
            ))
        });
        let browser_service = browser_service
            .unwrap_or_else(|| Arc::new(NodeBrowserService::discover(storage.data_dir())));
        let browser_verification_runner = browser_verification_runner.unwrap_or_else(|| {
            Arc::new(ServiceBrowserVerificationRunner::new(
                storage.clone(),
                browser_service.clone(),
            ))
        });
        if recovery.changed_state() {
            events.emit(
                "system.recovery",
                serde_json::to_value(&recovery).unwrap_or_default(),
            )?;
        }
        let state = Self {
            inner: Arc::new(Inner {
                started_at: Instant::now(),
                shutdown,
                events,
                jobs: JobSupervisor::with_storage(storage.database().clone()),
                terminals: TerminalRegistry::default(),
                agents: AgentRegistry::default(),
                sessions: SessionRegistry::default(),
                browser: BrowserHandle::default(),
                browser_service,
                browser_verification_runner,
                storage,
                filesystem: FilesystemHandle::default(),
                recovery,
                schema_version,
                permissions,
                verification_runner,
                dev_server_runtime,
                diagnostics,
                ide: ide.unwrap_or_else(|| Arc::new(SystemIdeIntegration)),
                runtime_observability_installed: AtomicBool::new(false),
            }),
        };
        Ok(state)
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
    pub fn browser_verification_runner(&self) -> &dyn BrowserVerificationRunner {
        self.inner.browser_verification_runner.as_ref()
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
    pub fn diagnostics_service(&self) -> &Arc<DiagnosticsService> {
        &self.inner.diagnostics
    }

    pub fn ide(&self) -> &dyn IdeIntegration {
        self.inner.ide.as_ref()
    }

    pub fn install_runtime_observability(&self) {
        self.inner
            .runtime_observability_installed
            .store(true, Ordering::Release);
        self.configure_runtime_observability();
    }

    pub fn configure_runtime_observability(&self) {
        if !self
            .inner
            .runtime_observability_installed
            .load(Ordering::Acquire)
        {
            return;
        }
        let diagnostics = self.inner.diagnostics.clone();
        let enabled = diagnostics.privacy().telemetry_enabled;
        retcon_runtime_observability::configure(Some(diagnostics), enabled);
    }

    pub fn disable_runtime_observability(&self) {
        self.inner
            .runtime_observability_installed
            .store(false, Ordering::Release);
        retcon_runtime_observability::configure(None, false);
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
            "durable_browser_verification": true,
            "local_diagnostics": true,
            "ide_integration": true,
        })
    }

    pub async fn diagnostics(&self) -> Value {
        crate::diagnostics_rpc::snapshot(self, 20).await
    }
}
