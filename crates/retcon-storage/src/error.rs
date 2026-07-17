//! Storage errors with enough context to drive a recovery UI.

use std::path::PathBuf;

/// A result returned by the storage layer.
pub type Result<T> = std::result::Result<T, StorageError>;

/// Failures encountered while opening or operating on durable storage.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// A filesystem operation failed.
    #[error("could not {operation} `{path}`: {source}")]
    Io {
        /// The operation being attempted.
        operation: &'static str,
        /// The affected path.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// SQLite rejected an operation.
    #[error("database operation `{operation}` failed: {source}")]
    Database {
        /// The operation being attempted.
        operation: &'static str,
        /// The underlying SQLite error.
        source: rusqlite::Error,
    },
    /// SQLite identified an invalid or damaged database file.
    #[error("database `{path}` is corrupted: {details}")]
    Corrupt {
        /// The damaged database path.
        path: PathBuf,
        /// Integrity-check output or the SQLite error message.
        details: String,
    },
    /// A newer Retcon version created this database.
    #[error("database schema version {found} is newer than supported version {supported}")]
    SchemaTooNew {
        /// Version stored in the database.
        found: u32,
        /// Newest version understood by this build.
        supported: u32,
    },
    /// The database connection lock was poisoned by a panic.
    #[error("database connection is unavailable after an internal failure")]
    ConnectionPoisoned,
    /// A backup destination already exists and will not be overwritten.
    #[error("backup destination `{0}` already exists")]
    BackupExists(PathBuf),
    /// An artifact hash is malformed or does not match its content.
    #[error("artifact integrity check failed for `{hash}`: {details}")]
    ArtifactIntegrity {
        /// The requested or computed SHA-256 hash.
        hash: String,
        /// Details suitable for diagnostics.
        details: String,
    },
    /// A caller supplied a task-planning value that violates a domain invariant.
    #[error("invalid persisted state: {0}")]
    Validation(String),
}

impl StorageError {
    pub(crate) fn database(operation: &'static str, source: rusqlite::Error) -> Self {
        Self::Database { operation, source }
    }

    pub(crate) fn io(
        operation: &'static str,
        path: impl Into<PathBuf>,
        source: std::io::Error,
    ) -> Self {
        Self::Io {
            operation,
            path: path.into(),
            source,
        }
    }
}
