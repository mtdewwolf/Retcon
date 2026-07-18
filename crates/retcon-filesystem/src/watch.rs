//! Directory watching with debounced, workspace-bounded change notifications.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use notify::event::{ModifyKind, RenameMode};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::FilesystemError;
use crate::service::{resolve_within_root, revision_for_file};

/// Quiet window used to coalesce noisy platform watcher notifications.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// Payload emitted when a watched path changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChangeEvent {
    /// Client-provided watch identifier.
    pub watch_id: String,
    /// Absolute path that changed.
    pub path: String,
    /// Previous absolute path for a paired rename.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_path: Option<String>,
    /// High-level change classification.
    pub change: String,
    /// Current SHA-256 revision for an extant regular file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

type EmitCallback = Arc<dyn Fn(FileChangeEvent) + Send + Sync>;

#[derive(Debug, Clone)]
struct PendingChange {
    path: PathBuf,
    previous_path: Option<PathBuf>,
    change: String,
}

struct WatchState {
    watcher: Option<RecommendedWatcher>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Drop for WatchState {
    fn drop(&mut self) {
        // Dropping the watcher closes the callback's last sender. The worker then exits without
        // emitting stale pending notifications after an explicit unwatch.
        self.watcher.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Registry of active directory watches.
#[derive(Clone, Default)]
pub struct WatchRegistry {
    inner: Arc<Mutex<HashMap<String, WatchState>>>,
}

impl WatchRegistry {
    /// Starts watching `path` under `root`, invoking `emit` for coalesced changes.
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

        let (sender, receiver) = mpsc::channel::<Vec<PendingChange>>();
        let watch_root = directory.clone();
        let mut watcher =
            notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
                let Ok(event) = result else {
                    return;
                };
                let changes = normalize_event(&watch_root, &event);
                if !changes.is_empty() {
                    let _ = sender.send(changes);
                }
            })
            .map_err(|error| FilesystemError::Watch(error.to_string()))?;

        watcher
            .watch(&directory, RecursiveMode::Recursive)
            .map_err(|error| FilesystemError::Watch(error.to_string()))?;

        let watch_id_for_worker = watch_id.clone();
        let worker = thread::spawn(move || {
            run_debouncer(receiver, watch_id_for_worker, emit);
        });
        let handle = FileWatchHandle {
            watch_id: watch_id.clone(),
            root: directory,
        };
        let previous = self
            .inner
            .lock()
            .map_err(|_| {
                FilesystemError::Watch("file watch registry lock is unavailable".to_owned())
            })?
            .insert(
                watch_id,
                WatchState {
                    watcher: Some(watcher),
                    worker: Some(worker),
                },
            );
        drop(previous);
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

fn run_debouncer(
    receiver: mpsc::Receiver<Vec<PendingChange>>,
    watch_id: String,
    emit: EmitCallback,
) {
    let mut pending = HashMap::<PathBuf, PendingChange>::new();
    loop {
        match receiver.recv_timeout(DEBOUNCE) {
            Ok(changes) => {
                for change in changes {
                    merge_change(&mut pending, change);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let mut ready = pending
                    .drain()
                    .map(|(_, change)| change)
                    .collect::<Vec<_>>();
                ready.sort_by(|left, right| left.path.cmp(&right.path));
                for change in ready {
                    let revision = if change.change == "deleted" {
                        None
                    } else {
                        revision_for_file(&change.path).ok().flatten()
                    };
                    emit(FileChangeEvent {
                        watch_id: watch_id.clone(),
                        path: change.path.to_string_lossy().into_owned(),
                        previous_path: change
                            .previous_path
                            .map(|path| path.to_string_lossy().into_owned()),
                        change: change.change,
                        revision,
                    });
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn merge_change(pending: &mut HashMap<PathBuf, PendingChange>, mut incoming: PendingChange) {
    if incoming.change == "renamed" && incoming.previous_path.is_none() {
        let incoming_exists = incoming.path.exists();
        let counterpart = pending.iter().find_map(|(path, change)| {
            (change.change == "renamed"
                && change.previous_path.is_none()
                && path.exists() != incoming_exists)
                .then(|| path.clone())
        });
        if let Some(counterpart) = counterpart
            && let Some(previous) = pending.remove(&counterpart)
        {
            if incoming_exists {
                incoming.previous_path = Some(previous.path);
            } else {
                let mut current = previous;
                current.previous_path = Some(incoming.path);
                pending.insert(current.path.clone(), current);
                return;
            }
        }
    }
    match pending.get_mut(&incoming.path) {
        Some(existing) => {
            existing.change = merge_change_kind(&existing.change, &incoming.change).to_owned();
            if incoming.previous_path.is_some() {
                existing.previous_path = incoming.previous_path;
            }
        }
        None => {
            pending.insert(incoming.path.clone(), incoming);
        }
    }
}

fn merge_change_kind(existing: &str, incoming: &str) -> &'static str {
    match (existing, incoming) {
        (_, "deleted") => "deleted",
        ("created", _) => "created",
        (_, "renamed") => "renamed",
        ("renamed", _) => "renamed",
        (_, "created") => "created",
        _ => "modified",
    }
}

fn normalize_event(root: &Path, event: &notify::Event) -> Vec<PendingChange> {
    if matches!(event.kind, EventKind::Access(_)) {
        return Vec::new();
    }
    if matches!(
        event.kind,
        EventKind::Modify(ModifyKind::Name(RenameMode::Both))
    ) && event.paths.len() >= 2
    {
        let previous = bounded_path(root, &event.paths[0]);
        let current = bounded_path(root, &event.paths[1]);
        return match (previous, current) {
            (Some(previous_path), Some(path)) => vec![PendingChange {
                path,
                previous_path: Some(previous_path),
                change: "renamed".to_owned(),
            }],
            (None, Some(path)) => vec![PendingChange {
                path,
                previous_path: None,
                change: "created".to_owned(),
            }],
            (Some(path), None) => vec![PendingChange {
                path,
                previous_path: None,
                change: "deleted".to_owned(),
            }],
            (None, None) => Vec::new(),
        };
    }

    let change = classify_change(&event.kind);
    event
        .paths
        .iter()
        .filter_map(|path| bounded_path(root, path))
        .map(|path| PendingChange {
            path,
            previous_path: None,
            change: change.to_owned(),
        })
        .collect()
}

fn bounded_path(root: &Path, path: &Path) -> Option<PathBuf> {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    if !normalized.starts_with(root) || is_internal_temporary(&normalized) {
        return None;
    }
    if normalized.exists() {
        let canonical = std::fs::canonicalize(&normalized).ok()?;
        if !canonical.starts_with(root) {
            return None;
        }
    }
    Some(normalized)
}

fn is_internal_temporary(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        let name = name.to_string_lossy();
        name.starts_with(".retcon-") && name.ends_with(".tmp")
    })
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

fn classify_change(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::Create(_) => "created",
        EventKind::Modify(ModifyKind::Name(_)) => "renamed",
        EventKind::Modify(_) => "modified",
        EventKind::Remove(_) => "deleted",
        EventKind::Any | EventKind::Other => "modified",
        EventKind::Access(_) => "modified",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, Instant};

    fn wait_for(
        events: &Arc<Mutex<Vec<FileChangeEvent>>>,
        predicate: impl Fn(&FileChangeEvent) -> bool,
    ) -> FileChangeEvent {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if let Some(event) = events
                .lock()
                .unwrap()
                .iter()
                .find(|event| predicate(event))
                .cloned()
            {
                return event;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for watcher event"
            );
            thread::sleep(Duration::from_millis(25));
        }
    }

    #[test]
    fn watcher_emits_revision_and_coalesces_rapid_writes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let path = root.join("example.txt");
        fs::write(&path, "initial").unwrap();
        let registry = WatchRegistry::default();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        registry
            .watch(
                "watch-1".to_owned(),
                root,
                None,
                Arc::new(move |event| captured.lock().unwrap().push(event)),
            )
            .unwrap();

        for content in ["one", "two", "three", "four"] {
            fs::write(&path, content).unwrap();
        }
        let event = wait_for(&events, |event| event.path.ends_with("example.txt"));
        assert_eq!(event.change, "modified");
        assert_eq!(event.revision.as_deref().map(str::len), Some(64));
        thread::sleep(Duration::from_millis(350));
        let count = events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.path.ends_with("example.txt"))
            .count();
        assert_eq!(count, 1);

        fs::write(root.join("created.txt"), "created").unwrap();
        let created = wait_for(&events, |event| {
            event.change == "created" && event.path.ends_with("created.txt")
        });
        assert_eq!(created.revision.as_deref().map(str::len), Some(64));
        assert!(registry.unwatch("watch-1"));
    }

    #[test]
    fn watcher_reports_paired_rename_and_delete() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let before = root.join("before.txt");
        let after = root.join("after.txt");
        fs::write(&before, "content").unwrap();
        let registry = WatchRegistry::default();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        registry
            .watch(
                "watch-2".to_owned(),
                root,
                None,
                Arc::new(move |event| captured.lock().unwrap().push(event)),
            )
            .unwrap();

        fs::rename(&before, &after).unwrap();
        let renamed = wait_for(&events, |event| {
            event.change == "renamed" && event.path.ends_with("after.txt")
        });
        assert!(
            renamed
                .previous_path
                .as_deref()
                .is_some_and(|path| path.ends_with("before.txt"))
        );
        assert_eq!(renamed.revision.as_deref().map(str::len), Some(64));

        fs::remove_file(&after).unwrap();
        let deleted = wait_for(&events, |event| {
            event.change == "deleted" && event.path.ends_with("after.txt")
        });
        assert!(deleted.revision.is_none());
        assert!(registry.unwatch("watch-2"));
    }
}
