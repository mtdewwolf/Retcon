//! The Retcon core service: a durable local process that supervises agents,
//! terminals, Git, storage, and the browser service.
//!
//! Phase 1 scope: process startup/shutdown skeleton with structured logging and
//! panic reporting. Lifecycle management, the event bus, and the job supervisor
//! land in Phase 3; the local protocol lands in Phase 4.

use std::io::IsTerminal;
use std::path::PathBuf;

mod checkpoints_rpc;
mod dev_servers_rpc;
pub mod error;
pub mod dev_servers;
pub mod event;
mod file_rpc;
pub mod frame;
mod git_rpc;
pub mod jobs;
pub mod lifecycle;
mod permissions_rpc;
pub mod projects;
mod projects_rpc;
mod providers_rpc;
pub mod rpc;
mod secrets_rpc;
pub mod server;
pub mod session_engine;
mod session_rpc;
pub mod spikes;
pub mod state;
mod storage_rpc;
mod tasks_rpc;
pub mod verification;
mod verification_rpc;

pub use error::{CoreError, ErrorCode, ErrorSource};
pub use lifecycle::{CoreConfig, CoreRuntime};

/// How log output is formatted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogFormat {
    /// Human-readable output for interactive terminals.
    Pretty,
    /// Newline-delimited JSON for ingestion and support bundles.
    Json,
}

/// The version of the core service, from the crate manifest.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Initialize the global `tracing` subscriber.
///
/// The filter is taken from `RETCON_LOG` (falling back to `RUST_LOG`, then
/// `info`). Returns an error message if a global subscriber is already set.
pub fn init_logging(format: LogFormat) -> Result<(), String> {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_env("RETCON_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(std::io::stderr().is_terminal())
        .with_writer(std::io::stderr);

    let result = match format {
        LogFormat::Pretty => builder.try_init(),
        LogFormat::Json => builder.json().try_init(),
    };
    result.map_err(|e| e.to_string())
}

/// Install a panic hook that reports panics through `tracing` before the
/// default hook runs, so crashes always appear in structured logs.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("<non-string panic payload>");
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".to_owned());
        tracing::error!(panic.message = payload, panic.location = %location, "core service panicked");
        default_hook(info);
    }));
}

/// Run the core service until a shutdown signal (Ctrl-C) is received.
///
/// # Errors
///
/// Returns an error message if the shutdown signal cannot be installed.
pub async fn run(data_dir: PathBuf) -> Result<(), CoreError> {
    tracing::info!(
        version = version(),
        pid = std::process::id(),
        "retcon-core started"
    );

    let runtime = CoreRuntime::start(CoreConfig::new(data_dir)).await?;
    tracing::info!(transport = %runtime.endpoint().path_string(), "retcon-core is ready");
    runtime.wait_for_shutdown_signal().await?;
    runtime.shutdown().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_manifest() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
        assert!(!version().is_empty());
    }
}
