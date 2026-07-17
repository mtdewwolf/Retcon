//! Checkpoints, snapshots, and rollback for Retcon.
//!
//! Captures file and Git state before mutating operations, stores content in
//! the CAS artifact store, and supports rollback preview plus selective restore.

#![allow(missing_docs)] // Phase 18 API; public documentation lands with the generated protocol.

mod error;
mod kind;
mod service;

pub use error::CheckpointError;
pub use kind::CheckpointKind;
pub use service::{
    CheckpointService, FileWriteCheckpoint, RestoreReport, RollbackPreview, RollbackPreviewItem,
};
