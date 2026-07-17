//! Checkpoint and rollback errors.

use retcon_git::GitError;
use retcon_storage::StorageError;

/// Failures while creating or restoring checkpoints.
#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    /// Storage or artifact failure.
    #[error("{0}")]
    Storage(#[from] StorageError),
    /// Git CLI failure while capturing a snapshot.
    #[error("{0}")]
    Git(#[from] GitError),
    /// The repository path is not registered with Retcon.
    #[error("repository path is not registered: {0}")]
    UnknownRepository(String),
    /// A checkpoint record was not found.
    #[error("checkpoint not found: {0}")]
    NotFound(String),
    /// A file path is outside the project root.
    #[error("path is outside the project root: {0}")]
    OutsideRoot(String),
    /// Rollback was blocked because unrelated user edits would be overwritten.
    #[error("rollback blocked by conflicts on: {0}")]
    Conflicts(String),
    /// Invalid request parameters.
    #[error("{0}")]
    InvalidRequest(String),
    /// Local filesystem failure while capturing or restoring files.
    #[error("filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
}
