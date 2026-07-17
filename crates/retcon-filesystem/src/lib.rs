//! File service: listing, reading, writing, and change watching.
//!
//! All operations are scoped to a project root to prevent path traversal.

pub mod error;
pub mod service;
pub mod watch;

pub use error::FilesystemError;
pub use service::{
    DEFAULT_READ_LIMIT, DEFAULT_WRITE_LIMIT, FileEntry, FileReadResult, FileService,
    FileWriteResult, LIST_MAX_ENTRIES, resolve_within_root,
};
pub use watch::{FileChangeEvent, FileWatchHandle, WatchRegistry, new_watch_id};

/// Shared filesystem service state used by the core RPC layer.
#[derive(Clone, Default)]
pub struct FilesystemHandle {
    service: FileService,
    watches: WatchRegistry,
}

impl FilesystemHandle {
    /// Returns the file service.
    #[must_use]
    pub fn service(&self) -> &FileService {
        &self.service
    }

    /// Returns the watch registry.
    #[must_use]
    pub fn watches(&self) -> &WatchRegistry {
        &self.watches
    }

    /// Stops active watches during shutdown.
    pub fn shutdown(&self) {
        self.watches.shutdown();
    }
}
