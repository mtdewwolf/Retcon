//! Shared state for the running core service.

#![allow(missing_docs)] // Phase 2 API; public documentation lands with the generated protocol.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::sync::watch;

#[derive(Clone)]
pub struct CoreState {
    inner: Arc<Inner>,
}

struct Inner {
    started_at: Instant,
    shutdown: watch::Sender<bool>,
}

impl CoreState {
    pub fn new() -> Self {
        let (shutdown, _) = watch::channel(false);
        Self {
            inner: Arc::new(Inner {
                started_at: Instant::now(),
                shutdown,
            }),
        }
    }

    pub fn uptime(&self) -> Duration {
        self.inner.started_at.elapsed()
    }

    pub fn request_shutdown(&self) {
        self.inner.shutdown.send_replace(true);
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
            "event_replay": false,
            "job_supervisor": false,
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

impl Default for CoreState {
    fn default() -> Self {
        Self::new()
    }
}
