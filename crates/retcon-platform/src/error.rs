//! Transport-level errors for local IPC endpoints.

use std::io;

use thiserror::Error;

/// Errors raised while binding or using a local transport endpoint.
#[derive(Debug, Error)]
pub enum TransportError {
    /// Underlying I/O failure.
    #[error("transport I/O: {context}: {source}")]
    Io {
        /// Human-readable operation context.
        context: &'static str,
        /// Original I/O error.
        #[source]
        source: io::Error,
    },
    /// The endpoint path is invalid for this platform.
    #[error("invalid transport path: {0}")]
    InvalidPath(String),
}

impl TransportError {
    pub(crate) fn io(context: &'static str, source: io::Error) -> Self {
        Self::Io { context, source }
    }
}
