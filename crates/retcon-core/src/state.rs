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
}

impl CoreState {
    pub fn new(data_dir: &std::path::Path) -> Result<Self, CoreError> {
        let (shutdown, _) = watch::channel(false);
        Ok(Self {
            inner: Arc::new(Inner {
                started_at: Instant::now(),
                shutdown,
                events: EventBus::open(data_dir)?,
                jobs: JobSupervisor::default(),
                terminals: TerminalRegistry::default(),
                agents: AgentRegistry::default(),
                browser: BrowserHandle::default(),
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
    pub fn browser(&self) -> &BrowserHandle {
        &self.inner.browser
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
        json!({
            "status": "healthy",
            "uptime_ms": self.uptime().as_millis(),
        })
    }

    pub fn capabilities(&self) -> Value {
        json!({
            "lifecycle": true,
            "structured_errors": true,
            "event_replay": true,
            "job_supervisor": true,
        })
    }

    pub fn diagnostics(&self) -> Value {
        json!({
            "process_id": std::process::id(),
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "uptime_ms": self.uptime().as_millis(),
            "version": env!("CARGO_PKG_VERSION"),
        })
    }
}
