//! Filesystem service errors.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Errors raised by the filesystem service.
#[derive(Debug, Error)]
pub enum FilesystemError {
    /// The requested path does not exist.
    #[error("path not found: {0}")]
    NotFound(PathBuf),
    /// The path escapes the configured project root.
    #[error("path escapes project root: {0}")]
    OutsideRoot(PathBuf),
    /// The file exceeds the configured read limit.
    #[error("file exceeds read limit ({size} bytes, limit {limit})")]
    TooLarge {
        /// Absolute path to the file.
        path: PathBuf,
        /// File size in bytes.
        size: u64,
        /// Configured limit in bytes.
        limit: u64,
    },
    /// The request parameters were invalid.
    #[error("{0}")]
    InvalidRequest(String),
    /// The requested conditional write no longer matches the file on disk.
    #[error("file changed since it was read")]
    Conflict {
        /// Revision currently present on disk, or `None` when the path is absent.
        current_revision: Option<String>,
    },
    /// An I/O error occurred.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// The filesystem watcher reported an error.
    #[error("watch error: {0}")]
    Watch(String),
}

impl FilesystemError {
    /// Returns `true` when the error represents a missing path.
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::NotFound(_) => true,
            Self::Io(error) => error.kind() == io::ErrorKind::NotFound,
            _ => false,
        }
    }
}
