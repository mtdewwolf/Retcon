//! Errors returned by the approval engine.

use retcon_storage::StorageError;

/// Failures while evaluating or recording permissions.
#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    #[error("approval not found")]
    ApprovalNotFound,
    #[error("approval already decided")]
    ApprovalAlreadyDecided,
    #[error("permission rule not found")]
    RuleNotFound,
    #[error(transparent)]
    Storage(#[from] StorageError),
}

pub type Result<T> = std::result::Result<T, PermissionError>;
