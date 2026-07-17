//! Directory watching with debounced change notifications.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::event::{ModifyKind, RenameMode};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::FilesystemError;
use crate::service::resolve_within_root;

/// Debounce window for coalescing filesystem notifications.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// Payload emitted when a watched path changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChangeEvent {
    /// Client-provided watch identifier.
    pub watch_id: String,
    /// Absolute path that changed.
    pub path: String,
    /// High-level change classification.
    pub change: String,
}

type EmitCallback = Arc<dyn Fn(FileChangeEvent) + Send + Sync>;

struct WatchState {
    _watcher: RecommendedWatcher,
    root: PathBuf,
    last_emit: HashMap<PathBuf, Instant>,
}

/// Registry of active directory watches.
#[derive(Clone, Default)]
pub struct WatchRegistry {
    inner: Arc<Mutex<HashMap<String, WatchState>>>,
}

impl WatchRegistry {
    /// Starts watching `path` under `root`, invoking `emit` for debounced changes.
    pub fn watch(
        &self,
        watch_id: String,
        root: &Path,
        path: Option<&str>,
        emit: EmitCallback,
    ) -> Result<FileWatchHandle, FilesystemError> {
        let directory = resolve_within_root(root, path.unwrap_or(""))?;
        if !directory.is_dir() {
            return Err(FilesystemError::InvalidRequest(format!(
                "watch path is not a directory: {}",
                directory.display()
            )));
        }

        let watch_id_for_callback = watch_id.clone();
        let registry = self.inner.clone();
        let mut watcher = notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
            let Ok(event) = result else {
                return;
            };
            let Some(path) = event.paths.first().cloned() else {
                return;
            };
            let change = classify_change(&event.kind);
            let should_emit = {
                let Ok(mut watches) = registry.lock() else {
                    return;
                };
                let Some(state) = watches.get_mut(&watch_id_for_callback) else {
                    return;
                };
                if !path.starts_with(&state.root) {
                    return;
                }
                let now = Instant::now();
                match state.last_emit.get(&path) {
                    Some(previous) if now.duration_since(*previous) < DEBOUNCE => false,
                    _ => {
                        state.last_emit.insert(path.clone(), now);
                        true
                    }
                }
            };
            if should_emit {
                emit(FileChangeEvent {
                    watch_id: watch_id_for_callback.clone(),
                    path: path.to_string_lossy().into_owned(),
                    change,
                });
            }
        })
        .map_err(|error| FilesystemError::Watch(error.to_string()))?;

        watcher
            .watch(&directory, RecursiveMode::Recursive)
            .map_err(|error| FilesystemError::Watch(error.to_string()))?;

        let handle = FileWatchHandle {
            watch_id: watch_id.clone(),
            root: directory.clone(),
        };
        if let Ok(mut watches) = self.inner.lock() {
            watches.insert(
                watch_id,
                WatchState {
                    _watcher: watcher,
                    root: directory,
                    last_emit: HashMap::new(),
                },
            );
        }
        Ok(handle)
    }

    /// Stops an active watch.
    pub fn unwatch(&self, watch_id: &str) -> bool {
        self.inner
            .lock()
            .ok()
            .and_then(|mut watches| watches.remove(watch_id))
            .is_some()
    }

    /// Stops every active watch.
    pub fn shutdown(&self) {
        if let Ok(mut watches) = self.inner.lock() {
            watches.clear();
        }
    }
}

/// Handle returned when a watch is registered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileWatchHandle {
    /// Client-provided watch identifier.
    pub watch_id: String,
    /// Absolute directory being watched.
    pub root: PathBuf,
}

impl FileWatchHandle {
    /// Serializes the handle for RPC responses.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "watchId": self.watch_id,
            "root": self.root.to_string_lossy(),
        })
    }
}

/// Creates a stable watch identifier when the client does not supply one.
#[must_use]
pub fn new_watch_id() -> String {
    Uuid::new_v4().to_string()
}

fn classify_change(kind: &EventKind) -> String {
    match kind {
        EventKind::Create(_) => "created".to_owned(),
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => "renamed".to_owned(),
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => "renamed".to_owned(),
        EventKind::Modify(_) => "modified".to_owned(),
        EventKind::Remove(_) => "deleted".to_owned(),
        EventKind::Any | EventKind::Access(_) | EventKind::Other => "changed".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::{fs, thread, time};

    #[test]
    fn watch_emits_on_write() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("src")).unwrap();

        let registry = WatchRegistry::default();
        let count = Arc::new(AtomicUsize::new(0));
        let count_for_emit = Arc::clone(&count);
        registry
            .watch(
                "watch-1".to_owned(),
                root,
                None,
                Arc::new(move |_event| {
                    count_for_emit.fetch_add(1, Ordering::SeqCst);
                }),
            )
            .unwrap();

        fs::write(root.join("src/example.txt"), "one").unwrap();
        thread::sleep(time::Duration::from_millis(300));
        fs::write(root.join("src/example.txt"), "two").unwrap();
        thread::sleep(time::Duration::from_millis(300));

        assert!(count.load(Ordering::SeqCst) >= 1);
        assert!(registry.unwatch("watch-1"));
    }
}
