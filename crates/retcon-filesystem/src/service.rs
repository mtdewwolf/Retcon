//! Directory listing, reading, and writing within a project root.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::FilesystemError;

static WRITE_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();

fn runtime_metrics_enabled() -> bool {
    retcon_runtime_observability::is_enabled()
}

struct FilesystemMetric {
    operation: &'static str,
    started: Instant,
    outcome: &'static str,
}

impl FilesystemMetric {
    fn new(operation: &'static str) -> Self {
        Self {
            operation,
            started: Instant::now(),
            outcome: "error",
        }
    }

    fn succeed(&mut self) {
        self.outcome = "ok";
    }
}

impl Drop for FilesystemMetric {
    fn drop(&mut self) {
        retcon_runtime_observability::record_duration(
            "filesystem",
            "filesystem.operation.duration",
            self.operation,
            self.outcome,
            self.started.elapsed(),
        );
        if !runtime_metrics_enabled() {
            return;
        }
        tracing::info!(
            target: "retcon_runtime",
            event = "filesystem.operation.completed",
            component = "filesystem",
            operation = self.operation,
            outcome = self.outcome,
            duration_ms = self.started.elapsed().as_millis() as u64
        );
    }
}

/// Maximum directory entries returned by [`FileService::list`].
pub const LIST_MAX_ENTRIES: usize = 2_048;

/// Default maximum bytes read by [`FileService::read`].
pub const DEFAULT_READ_LIMIT: u64 = 512 * 1024;

/// Maximum bytes accepted by [`FileService::write`].
pub const DEFAULT_WRITE_LIMIT: u64 = 2 * 1024 * 1024;

/// A single entry in a directory listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    /// Entry name (not a full path).
    pub name: String,
    /// Absolute path on disk.
    pub path: String,
    /// Whether this entry is a directory.
    pub is_directory: bool,
    /// File size in bytes (zero for directories).
    pub size: u64,
    /// Optional git status badge derived from porcelain (`modified` / `added` / `deleted`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_status: Option<String>,
}

/// Result of reading a file through the service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReadResult {
    /// Absolute path on disk.
    pub path: String,
    /// UTF-8 text content when the file is textual.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Base64 payload when the file is binary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_base64: Option<String>,
    /// Full file size in bytes.
    pub size: u64,
    /// Whether the payload was truncated to the read limit.
    pub truncated: bool,
    /// Whether the payload is treated as binary.
    pub binary: bool,
    /// Detected language hint for syntax highlighting.
    pub language: String,
    /// SHA-256 revision of the complete file content.
    pub revision: String,
}

/// Required optimistic-concurrency condition for a file write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileWriteCondition {
    /// Replace an existing file only when its revision still matches.
    IfMatch(String),
    /// Create a new file only when no file exists at the requested path.
    IfNoneMatch,
}

/// Filesystem operations scoped to a project root.
#[derive(Debug, Clone, Default)]
pub struct FileService;

impl FileService {
    /// Lists a single directory level under `root`.
    ///
    /// When `root` is inside a Git work tree, each entry's `git_status` reflects
    /// real porcelain status (directories inherit a badge if any child is dirty).
    pub async fn list(
        &self,
        root: &Path,
        path: Option<&str>,
    ) -> Result<Vec<FileEntry>, FilesystemError> {
        let mut metric = FilesystemMetric::new("list");
        let directory = resolve_within_root(root, path.unwrap_or(""))?;
        if !directory.is_dir() {
            return Err(FilesystemError::InvalidRequest(format!(
                "path is not a directory: {}",
                directory.display()
            )));
        }

        let git_index = load_git_status_index(root).await;
        let root_canon = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());

        let mut entries = Vec::new();
        for entry in fs::read_dir(&directory)? {
            if entries.len() >= LIST_MAX_ENTRIES {
                break;
            }
            let entry = entry?;
            let file_type = entry.file_type()?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') && name != ".gitignore" {
                continue;
            }
            let size = if file_type.is_dir() {
                0
            } else {
                entry.metadata().map(|meta| meta.len()).unwrap_or(0)
            };
            let is_directory = file_type.is_dir();
            let git_status = git_status_for_path(&root_canon, &path, is_directory, &git_index);
            entries.push(FileEntry {
                name,
                path: path.to_string_lossy().into_owned(),
                is_directory,
                size,
                git_status,
            });
        }

        entries.sort_by(
            |left, right| match (left.is_directory, right.is_directory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
            },
        );
        metric.succeed();
        Ok(entries)
    }

    /// Reads a file under `root`, truncating payloads larger than `limit`.
    pub fn read(
        &self,
        root: &Path,
        path: &str,
        limit: u64,
    ) -> Result<FileReadResult, FilesystemError> {
        let mut metric = FilesystemMetric::new("read");
        let file_path = resolve_within_root(root, path)?;
        if file_path.is_dir() {
            return Err(FilesystemError::InvalidRequest(format!(
                "path is a directory: {}",
                file_path.display()
            )));
        }
        let metadata = fs::metadata(&file_path)?;
        let size = metadata.len();
        if size > limit && limit == 0 {
            return Err(FilesystemError::TooLarge {
                path: file_path.clone(),
                size,
                limit,
            });
        }

        let (bytes, actual_size, revision) = read_with_revision(&file_path, limit)?;
        let truncated = actual_size > limit;
        let binary = is_probably_binary(&bytes);
        let language = language_for_path(&file_path);
        let (content, content_base64) = if binary {
            (None, Some(base64_encode(&bytes)))
        } else {
            (Some(String::from_utf8_lossy(&bytes).into_owned()), None)
        };

        let result = FileReadResult {
            path: file_path.to_string_lossy().into_owned(),
            content,
            content_base64,
            size: actual_size,
            truncated,
            binary,
            language,
            revision,
        };
        metric.succeed();
        Ok(result)
    }

    /// Writes UTF-8 text to a file under `root`.
    pub fn write(
        &self,
        root: &Path,
        path: &str,
        content: &str,
        limit: u64,
        condition: FileWriteCondition,
    ) -> Result<FileWriteResult, FilesystemError> {
        let mut metric = FilesystemMetric::new("write");
        let bytes = content.as_bytes();
        if bytes.len() as u64 > limit {
            return Err(FilesystemError::InvalidRequest(format!(
                "write payload exceeds limit ({}/{limit})",
                bytes.len()
            )));
        }
        let root = canonical_root(root)?;
        let file_path = resolve_within_canonical_root(&root, path)?;
        let path_lock = write_lock(&file_path)?;
        let _write_guard = path_lock.lock().map_err(|_| {
            FilesystemError::InvalidRequest("file write lock is unavailable".to_owned())
        })?;
        if file_path.is_dir() {
            return Err(FilesystemError::InvalidRequest(format!(
                "path is a directory: {}",
                file_path.display()
            )));
        }
        verify_write_condition(&file_path, &condition)?;
        let parent = file_path.parent().ok_or_else(|| {
            FilesystemError::InvalidRequest("file path has no parent directory".to_owned())
        })?;
        ensure_directories_within_root(&root, parent)?;
        let parent_identity = fs::canonicalize(parent)?;
        // Re-resolve after directory creation to catch a symlink/junction introduced by a race.
        let file_path = resolve_within_canonical_root(&root, path)?;

        let temp_path = parent.join(format!(".retcon-{}.tmp", Uuid::new_v4()));
        let mut cleanup = TempFileGuard::new(temp_path.clone());
        let mut staged = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        if let Ok(metadata) = fs::metadata(&file_path) {
            staged.set_permissions(metadata.permissions())?;
        }
        staged.write_all(bytes)?;
        staged.flush()?;
        staged.sync_all()?;
        drop(staged);

        // This is intentionally the last operation before commit. A stale writer never mutates
        // the destination, and create races are resolved by an atomic hard-link insertion.
        let file_path = resolve_within_canonical_root(&root, path)?;
        if fs::canonicalize(parent)? != parent_identity {
            return Err(FilesystemError::OutsideRoot(parent.to_path_buf()));
        }
        verify_write_condition(&file_path, &condition)?;
        match condition {
            FileWriteCondition::IfMatch(_) => atomic_replace(&temp_path, &file_path)?,
            FileWriteCondition::IfNoneMatch => {
                fs::hard_link(&temp_path, &file_path).map_err(|error| {
                    if error.kind() == std::io::ErrorKind::AlreadyExists {
                        FilesystemError::Conflict {
                            current_revision: revision_for_file(&file_path).ok().flatten(),
                        }
                    } else {
                        FilesystemError::Io(error)
                    }
                })?;
                let _ = fs::remove_file(&temp_path);
            }
        }
        cleanup.disarm();
        sync_parent(parent)?;
        let size = fs::metadata(&file_path)?.len();
        let revision = revision_for_file(&file_path)?
            .ok_or_else(|| FilesystemError::NotFound(file_path.clone()))?;
        let result = FileWriteResult {
            path: file_path.to_string_lossy().into_owned(),
            size,
            revision,
        };
        metric.succeed();
        Ok(result)
    }
}

fn write_lock(path: &Path) -> Result<Arc<Mutex<()>>, FilesystemError> {
    let locks = WRITE_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks.lock().map_err(|_| {
        FilesystemError::InvalidRequest("file write lock registry is unavailable".to_owned())
    })?;
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(path).and_then(Weak::upgrade) {
        return Ok(lock);
    }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(path.to_path_buf(), Arc::downgrade(&lock));
    Ok(lock)
}

/// Result of writing a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileWriteResult {
    /// Absolute path on disk.
    pub path: String,
    /// Resulting file size in bytes.
    pub size: u64,
    /// SHA-256 revision of the resulting complete file content.
    pub revision: String,
}

/// Resolves `path` under `root`, rejecting traversal outside the root.
pub fn resolve_within_root(root: &Path, path: &str) -> Result<PathBuf, FilesystemError> {
    let root = canonical_root(root)?;
    resolve_within_canonical_root(&root, path)
}

fn canonical_root(root: &Path) -> Result<PathBuf, FilesystemError> {
    fs::canonicalize(root).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            FilesystemError::NotFound(root.to_path_buf())
        } else {
            FilesystemError::Io(error)
        }
    })
}

fn resolve_within_canonical_root(root: &Path, path: &str) -> Result<PathBuf, FilesystemError> {
    if path.is_empty() {
        return Ok(root.to_path_buf());
    }

    let requested = Path::new(path);
    let joined = if requested.is_absolute() {
        normalize_path(requested)
    } else {
        normalize_path(&root.join(requested))
    };

    if !joined.starts_with(root) {
        return Err(FilesystemError::OutsideRoot(joined));
    }

    reject_symlink_traversal(root, &joined)?;
    Ok(joined)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn read_with_revision(path: &Path, limit: u64) -> Result<(Vec<u8>, u64, String), FilesystemError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut returned = Vec::with_capacity(limit.min(64 * 1024) as usize);
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        let remaining = limit.saturating_sub(returned.len() as u64) as usize;
        returned.extend_from_slice(&buffer[..read.min(remaining)]);
        size = size.saturating_add(read as u64);
    }
    Ok((returned, size, format!("{:x}", hasher.finalize())))
}

/// Computes the revision of a regular file, returning `None` when it is absent.
pub(crate) fn revision_for_file(path: &Path) -> Result<Option<String>, FilesystemError> {
    match File::open(path) {
        Ok(mut file) => {
            let mut hasher = Sha256::new();
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            Ok(Some(format!("{:x}", hasher.finalize())))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn verify_write_condition(
    path: &Path,
    condition: &FileWriteCondition,
) -> Result<(), FilesystemError> {
    let current_revision = revision_for_file(path)?;
    let matches = match condition {
        FileWriteCondition::IfMatch(expected) => current_revision.as_ref() == Some(expected),
        FileWriteCondition::IfNoneMatch => current_revision.is_none(),
    };
    if matches {
        Ok(())
    } else {
        Err(FilesystemError::Conflict { current_revision })
    }
}

fn reject_symlink_traversal(root: &Path, path: &Path) -> Result<(), FilesystemError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| FilesystemError::OutsideRoot(path.to_path_buf()))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(FilesystemError::OutsideRoot(path.to_path_buf()));
        }
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(FilesystemError::OutsideRoot(current));
                }
                let canonical = fs::canonicalize(&current)?;
                if !canonical.starts_with(root) {
                    return Err(FilesystemError::OutsideRoot(canonical));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn ensure_directories_within_root(root: &Path, directory: &Path) -> Result<(), FilesystemError> {
    let relative = directory
        .strip_prefix(root)
        .map_err(|_| FilesystemError::OutsideRoot(directory.to_path_buf()))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(FilesystemError::OutsideRoot(directory.to_path_buf()));
        };
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(FilesystemError::OutsideRoot(current));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current)?;
            }
            Err(error) => return Err(error.into()),
        }
        let canonical = fs::canonicalize(&current)?;
        if !canonical.starts_with(root) {
            return Err(FilesystemError::OutsideRoot(canonical));
        }
    }
    Ok(())
}

struct TempFileGuard {
    path: PathBuf,
    armed: bool,
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(unix)]
fn atomic_replace(from: &Path, to: &Path) -> Result<(), FilesystemError> {
    fs::rename(from, to)?;
    Ok(())
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn atomic_replace(from: &Path, to: &Path) -> Result<(), FilesystemError> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }

    let from = from
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let to = to
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: both pointers reference NUL-terminated UTF-16 buffers that remain alive for the call.
    let result = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn atomic_replace(from: &Path, to: &Path) -> Result<(), FilesystemError> {
    fs::rename(from, to)?;
    Ok(())
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), FilesystemError> {
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> Result<(), FilesystemError> {
    Ok(())
}

fn is_probably_binary(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    if bytes.contains(&0) {
        return true;
    }
    let sample = bytes.len().min(8192);
    let non_text = bytes[..sample]
        .iter()
        .filter(|byte| matches!(**byte, 0..=8 | 14..=31))
        .count();
    non_text * 10 > sample
}

fn language_for_path(path: &Path) -> String {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("rs") => "rust",
        Some("dart") => "dart",
        Some("js") | Some("mjs") | Some("cjs") => "javascript",
        Some("ts") | Some("tsx") => "typescript",
        Some("json") => "json",
        Some("yaml") | Some("yml") => "yaml",
        Some("toml") => "toml",
        Some("md") => "markdown",
        Some("py") => "python",
        Some("go") => "go",
        Some("css") => "css",
        Some("html") | Some("htm") => "html",
        Some("sql") => "sql",
        Some("sh") | Some("bash") | Some("ps1") => "shell",
        _ => "plaintext",
    }
    .to_owned()
}

async fn load_git_status_index(root: &Path) -> HashMap<String, String> {
    match retcon_git::status(root).await {
        Ok(status) => build_git_status_index(&status.entries),
        Err(_) => HashMap::new(),
    }
}

fn build_git_status_index(entries: &[retcon_git::StatusEntry]) -> HashMap<String, String> {
    let mut index: HashMap<String, String> = HashMap::new();
    for entry in entries {
        let Some(badge) = porcelain_code_to_badge(&entry.code) else {
            continue;
        };
        let path = entry.path.trim_end_matches('/').replace('\\', "/");
        if path.is_empty() {
            continue;
        }
        match index.get_mut(&path) {
            Some(existing) => {
                *existing = merge_git_badge(existing, badge).to_owned();
            }
            None => {
                index.insert(path, badge.to_owned());
            }
        }
    }
    index
}

fn git_status_for_path(
    root: &Path,
    path: &Path,
    is_directory: bool,
    index: &HashMap<String, String>,
) -> Option<String> {
    if index.is_empty() {
        return None;
    }
    let abs = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let rel = abs.strip_prefix(root).ok()?;
    let key = path_to_git_key(rel);
    if key.is_empty() {
        return None;
    }
    if let Some(badge) = index.get(&key) {
        return Some(badge.clone());
    }
    // Git collapses untracked directories to `?? dir/`; children inherit that badge.
    let mut ancestor = key.as_str();
    while let Some((parent, _)) = ancestor.rsplit_once('/') {
        if let Some(badge) = index.get(parent) {
            return Some(badge.clone());
        }
        ancestor = parent;
    }
    if !is_directory {
        return None;
    }
    let prefix = format!("{key}/");
    let mut best: Option<&str> = None;
    for (path, badge) in index {
        if path.starts_with(&prefix) {
            best = Some(match best {
                Some(current) => merge_git_badge(current, badge),
                None => badge.as_str(),
            });
        }
    }
    best.map(str::to_owned)
}

fn path_to_git_key(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Map a two-character porcelain code onto explorer badge labels.
fn porcelain_code_to_badge(code: &str) -> Option<&'static str> {
    let bytes = code.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let left = porcelain_char_to_badge(bytes[0]);
    let right = porcelain_char_to_badge(bytes[1]);
    match (left, right) {
        (Some(a), Some(b)) => Some(merge_git_badge(a, b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn porcelain_char_to_badge(code: u8) -> Option<&'static str> {
    match code {
        b'M' | b'R' | b'C' | b'U' => Some("modified"),
        b'A' | b'?' => Some("added"),
        b'D' => Some("deleted"),
        _ => None,
    }
}

fn merge_git_badge(left: &str, right: &str) -> &'static str {
    fn rank(badge: &str) -> u8 {
        match badge {
            "deleted" => 0,
            "modified" => 1,
            "added" => 2,
            _ => 3,
        }
    }
    let winner = if rank(left) <= rank(right) {
        left
    } else {
        right
    };
    match winner {
        "deleted" => "deleted",
        "added" => "added",
        _ => "modified",
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        encoded.push(TABLE[((triple >> 18) & 63) as usize] as char);
        encoded.push(TABLE[((triple >> 12) & 63) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 63) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            TABLE[(triple & 63) as usize] as char
        } else {
            '='
        });
    }
    encoded
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_and_read_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(root.join("README.md"), "# hello").unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();

        let service = FileService;
        let entries = service.list(root, None).await.unwrap();
        assert!(entries.iter().any(|entry| entry.name == "README.md"));
        assert!(
            entries
                .iter()
                .any(|entry| entry.name == "src" && entry.is_directory)
        );
        assert!(entries.iter().all(|entry| entry.git_status.is_none()));

        let read = service.read(root, "README.md", DEFAULT_READ_LIMIT).unwrap();
        assert_eq!(read.content.as_deref(), Some("# hello"));
        assert_eq!(read.language, "markdown");
        assert_eq!(read.revision.len(), 64);
    }

    #[tokio::test]
    async fn rejects_paths_outside_root() {
        let temp = tempfile::tempdir().unwrap();
        let service = FileService;
        let error = service
            .list(temp.path(), Some("../outside"))
            .await
            .expect_err("should reject traversal");
        assert!(matches!(error, FilesystemError::OutsideRoot(_)));
    }

    #[test]
    fn conditional_write_persists_changes() {
        let temp = tempfile::tempdir().unwrap();
        let service = FileService;
        fs::write(temp.path().join("notes.txt"), "original").unwrap();
        let revision = service
            .read(temp.path(), "notes.txt", DEFAULT_READ_LIMIT)
            .unwrap()
            .revision;
        let result = service
            .write(
                temp.path(),
                "notes.txt",
                "updated",
                DEFAULT_WRITE_LIMIT,
                FileWriteCondition::IfMatch(revision),
            )
            .unwrap();
        assert_eq!(result.size, 7);
        assert_eq!(result.revision.len(), 64);
        let read = service
            .read(temp.path(), "notes.txt", DEFAULT_READ_LIMIT)
            .unwrap();
        assert_eq!(read.content.as_deref(), Some("updated"));
        assert_eq!(read.revision, result.revision);
    }

    #[test]
    fn stale_writer_conflicts_without_changing_disk() {
        let temp = tempfile::tempdir().unwrap();
        let service = FileService;
        fs::write(temp.path().join("notes.txt"), "first").unwrap();
        let stale = service
            .read(temp.path(), "notes.txt", DEFAULT_READ_LIMIT)
            .unwrap()
            .revision;
        fs::write(temp.path().join("notes.txt"), "external change").unwrap();
        let current = service
            .read(temp.path(), "notes.txt", DEFAULT_READ_LIMIT)
            .unwrap()
            .revision;

        let error = service
            .write(
                temp.path(),
                "notes.txt",
                "stale writer",
                DEFAULT_WRITE_LIMIT,
                FileWriteCondition::IfMatch(stale),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            FilesystemError::Conflict {
                current_revision: Some(revision)
            } if revision == current
        ));
        assert_eq!(
            fs::read_to_string(temp.path().join("notes.txt")).unwrap(),
            "external change"
        );
    }

    #[test]
    fn concurrent_create_has_exactly_one_winner() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let temp = tempfile::tempdir().unwrap();
        let root = Arc::new(temp.path().to_path_buf());
        let barrier = Arc::new(Barrier::new(3));
        let mut writers = Vec::new();
        for content in ["writer one", "writer two"] {
            let root = Arc::clone(&root);
            let barrier = Arc::clone(&barrier);
            writers.push(thread::spawn(move || {
                barrier.wait();
                FileService.write(
                    &root,
                    "created.txt",
                    content,
                    DEFAULT_WRITE_LIMIT,
                    FileWriteCondition::IfNoneMatch,
                )
            }));
        }
        barrier.wait();
        let results = writers
            .into_iter()
            .map(|writer| writer.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(FilesystemError::Conflict { .. })))
                .count(),
            1
        );
        let content = fs::read_to_string(root.join("created.txt")).unwrap();
        assert!(content == "writer one" || content == "writer two");
        assert!(fs::read_dir(&*root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".retcon-")
        }));
    }

    #[cfg(unix)]
    #[test]
    fn repeated_atomic_replacement_never_exposes_partial_content() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::thread;

        let temp = tempfile::tempdir().unwrap();
        let root = Arc::new(temp.path().to_path_buf());
        let path = root.join("atomic.txt");
        let first = "a".repeat(128 * 1024);
        let second = "b".repeat(128 * 1024);
        fs::write(&path, &first).unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let reader_root = Arc::clone(&root);
        let reader_running = Arc::clone(&running);
        let first_for_reader = first.clone();
        let second_for_reader = second.clone();
        let reader = thread::spawn(move || {
            while reader_running.load(Ordering::Acquire) {
                let bytes = fs::read(reader_root.join("atomic.txt")).unwrap();
                assert!(
                    bytes == first_for_reader.as_bytes() || bytes == second_for_reader.as_bytes()
                );
            }
        });

        let service = FileService;
        for index in 0..12 {
            let read = service
                .read(&root, "atomic.txt", DEFAULT_WRITE_LIMIT)
                .unwrap();
            let content = if index % 2 == 0 { &second } else { &first };
            service
                .write(
                    &root,
                    "atomic.txt",
                    content,
                    DEFAULT_WRITE_LIMIT,
                    FileWriteCondition::IfMatch(read.revision),
                )
                .unwrap();
        }
        running.store(false, Ordering::Release);
        reader.join().unwrap();
    }

    #[test]
    fn staged_failure_guard_removes_temporary_file() {
        let temp = tempfile::tempdir().unwrap();
        let staged = temp.path().join(".retcon-test.tmp");
        fs::write(&staged, "staged").unwrap();
        {
            let _guard = TempFileGuard::new(staged.clone());
            // Dropping the guard models every early return after staging and before commit.
        }
        assert!(!staged.exists());
    }

    #[cfg(windows)]
    #[test]
    fn failed_atomic_replacement_preserves_destination_and_cleans_stage() {
        use std::os::windows::fs::OpenOptionsExt;

        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("destination.txt");
        let staged = temp.path().join(".retcon-failure.tmp");
        fs::write(&destination, "original").unwrap();
        fs::write(&staged, "replacement").unwrap();
        let guard = TempFileGuard::new(staged.clone());
        let locked = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&destination)
            .unwrap();

        assert!(atomic_replace(&staged, &destination).is_err());
        drop(locked);
        drop(guard);
        assert_eq!(fs::read_to_string(destination).unwrap(), "original");
        assert!(!staged.exists());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "outside").unwrap();
        symlink(outside.path(), root.path().join("escape")).unwrap();
        let error = FileService
            .read(root.path(), "escape/secret.txt", DEFAULT_READ_LIMIT)
            .unwrap_err();
        assert!(matches!(error, FilesystemError::OutsideRoot(_)));
    }

    #[cfg(windows)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::windows::fs::symlink_dir;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "outside").unwrap();
        if symlink_dir(outside.path(), root.path().join("escape")).is_err() {
            return;
        }
        let error = FileService
            .read(root.path(), "escape/secret.txt", DEFAULT_READ_LIMIT)
            .unwrap_err();
        assert!(matches!(error, FilesystemError::OutsideRoot(_)));
    }

    #[test]
    fn large_reads_are_truncated() {
        let temp = tempfile::tempdir().unwrap();
        let payload = "x".repeat(32);
        fs::write(temp.path().join("big.txt"), &payload).unwrap();
        let service = FileService;
        let read = service.read(temp.path(), "big.txt", 8).unwrap();
        let full = service
            .read(temp.path(), "big.txt", DEFAULT_READ_LIMIT)
            .unwrap();
        assert!(read.truncated);
        assert_eq!(read.content.as_deref(), Some("xxxxxxxx"));
        assert_eq!(read.size, 32);
        assert_eq!(read.revision, full.revision);
    }

    #[test]
    fn porcelain_codes_map_to_badges() {
        assert_eq!(porcelain_code_to_badge(" M"), Some("modified"));
        assert_eq!(porcelain_code_to_badge("M "), Some("modified"));
        assert_eq!(porcelain_code_to_badge("A "), Some("added"));
        assert_eq!(porcelain_code_to_badge("??"), Some("added"));
        assert_eq!(porcelain_code_to_badge(" D"), Some("deleted"));
        assert_eq!(porcelain_code_to_badge("AD"), Some("deleted"));
        assert_eq!(porcelain_code_to_badge("!!"), None);
    }

    #[test]
    fn status_index_matches_files_and_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join("notes.txt"), "notes\n").unwrap();
        fs::write(root.join("clean.txt"), "clean\n").unwrap();
        let root = fs::canonicalize(root).unwrap();

        let entries = [
            retcon_git::StatusEntry {
                code: " M".into(),
                path: "src/main.rs".into(),
            },
            retcon_git::StatusEntry {
                code: "??".into(),
                path: "notes.txt".into(),
            },
        ];
        let index = build_git_status_index(&entries);
        assert_eq!(
            git_status_for_path(&root, &root.join("notes.txt"), false, &index).as_deref(),
            Some("added")
        );
        assert_eq!(
            git_status_for_path(&root, &root.join("src"), true, &index).as_deref(),
            Some("modified")
        );
        assert_eq!(
            git_status_for_path(&root, &root.join("src/main.rs"), false, &index).as_deref(),
            Some("modified")
        );
        assert_eq!(
            git_status_for_path(&root, &root.join("clean.txt"), false, &index),
            None
        );
    }

    #[tokio::test]
    async fn list_uses_real_git_porcelain() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        retcon_git::run_git(root, &["init", "-b", "main"])
            .await
            .unwrap();
        retcon_git::run_git(root, &["config", "user.email", "retcon@example.invalid"])
            .await
            .unwrap();
        retcon_git::run_git(root, &["config", "user.name", "Retcon Test"])
            .await
            .unwrap();
        fs::write(root.join("tracked.txt"), "one\n").unwrap();
        retcon_git::run_git(root, &["add", "."]).await.unwrap();
        retcon_git::run_git(root, &["commit", "-m", "initial"])
            .await
            .unwrap();
        fs::write(root.join("tracked.txt"), "two\n").unwrap();
        fs::write(root.join("fresh.txt"), "new\n").unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/nested.txt"), "nested\n").unwrap();

        let service = FileService;
        let entries = service.list(root, None).await.unwrap();
        let tracked = entries.iter().find(|entry| entry.name == "tracked.txt");
        assert_eq!(
            tracked.and_then(|entry| entry.git_status.as_deref()),
            Some("modified")
        );
        let fresh = entries.iter().find(|entry| entry.name == "fresh.txt");
        assert_eq!(
            fresh.and_then(|entry| entry.git_status.as_deref()),
            Some("added")
        );
        let src = entries.iter().find(|entry| entry.name == "src");
        assert_eq!(
            src.and_then(|entry| entry.git_status.as_deref()),
            Some("added")
        );

        let nested = service.list(root, Some("src")).await.unwrap();
        let nested_file = nested.iter().find(|entry| entry.name == "nested.txt");
        assert_eq!(
            nested_file.and_then(|entry| entry.git_status.as_deref()),
            Some("added")
        );
    }
}
