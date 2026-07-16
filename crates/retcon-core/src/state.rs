//! Shared state for the running core service.

#![allow(missing_docs)] // Phase 2 API; public documentation lands with the generated protocol.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::sync::watch;

use crate::CoreError;
use crate::event::EventBus;
use crate::jobs::JobSupervisor;
use crate::spikes::agent::AgentRegistry;
use crate::spikes::browser::BrowserHandle;
use crate::spikes::terminal::TerminalRegistry;
use retcon_storage::{ArtifactStore, Database, RecoveryReport};

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
    browser: BrowserHandle,
    storage: Database,
    artifacts: ArtifactStore,
    recovery: RecoveryReport,
    schema_version: Option<u32>,
}

impl CoreState {
    pub fn new(data_dir: &std::path::Path) -> Result<Self, CoreError> {
        let (shutdown, _) = watch::channel(false);
        let storage = Database::open(data_dir.join("retcon.db"))?;
        let schema_version = storage.schema_version().ok();
        let recovery = storage.recover_interrupted()?;
        let artifacts = ArtifactStore::open(data_dir)?;
        let events = EventBus::open(storage.clone())?;
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
                jobs: JobSupervisor::with_storage(storage.clone()),
                terminals: TerminalRegistry::default(),
                agents: AgentRegistry::default(),
                browser: BrowserHandle::default(),
                storage,
                artifacts,
                recovery,
                schema_version,
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

    /// Broadcast a high-frequency event without durable SQLite persistence.
    pub fn emit_volatile(&self, kind: &str, payload: Value) {
        if let Err(error) = self.inner.events.emit_volatile(kind, payload) {
            tracing::error!(error_id=%error.id, "failed to emit volatile event");
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
    pub fn browser(&self) -> &BrowserHandle {
        &self.inner.browser
    }
    pub fn storage(&self) -> &Database {
        &self.inner.storage
    }
    pub fn artifacts(&self) -> &ArtifactStore {
        &self.inner.artifacts
    }
    pub fn recovery(&self) -> &RecoveryReport {
        &self.inner.recovery
    }

    pub async fn cleanup_children(&self) {
        self.inner.terminals.shutdown();
        self.inner.agents.shutdown().await;
        self.inner.browser.shutdown().await;
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
        })
    }

    pub async fn diagnostics(&self) -> Value {
        let artifact_bytes = self.inner.artifacts.disk_usage_async().await.ok();
        json!({
            "process_id": std::process::id(),
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "uptime_ms": self.uptime().as_millis(),
            "version": env!("CARGO_PKG_VERSION"),
            "storage": {
                "database": self.inner.storage.path(),
                "artifact_bytes": artifact_bytes,
                "recovery": self.inner.recovery,
            },
        })
    }
}
