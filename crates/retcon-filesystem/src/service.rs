//! Directory listing, reading, and writing within a project root.

use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::error::FilesystemError;

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

        let truncated = size > limit;
        let read_len = if truncated { limit } else { size } as usize;
        let bytes = if truncated {
            read_prefix(&file_path, read_len)?
        } else {
            fs::read(&file_path)?
        };
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
            size,
            truncated,
            binary,
            language,
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
    ) -> Result<FileWriteResult, FilesystemError> {
        let mut metric = FilesystemMetric::new("write");
        let bytes = content.as_bytes();
        if bytes.len() as u64 > limit {
            return Err(FilesystemError::InvalidRequest(format!(
                "write payload exceeds limit ({}/{limit})",
                bytes.len()
            )));
        }
        let file_path = resolve_within_root(root, path)?;
        if file_path.is_dir() {
            return Err(FilesystemError::InvalidRequest(format!(
                "path is a directory: {}",
                file_path.display()
            )));
        }
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&file_path, bytes)?;
        let size = fs::metadata(&file_path)?.len();
        let result = FileWriteResult {
            path: file_path.to_string_lossy().into_owned(),
            size,
        };
        metric.succeed();
        Ok(result)
    }
}

/// Result of writing a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileWriteResult {
    /// Absolute path on disk.
    pub path: String,
    /// Resulting file size in bytes.
    pub size: u64,
}

/// Resolves `path` under `root`, rejecting traversal outside the root.
pub fn resolve_within_root(root: &Path, path: &str) -> Result<PathBuf, FilesystemError> {
    let root = fs::canonicalize(root).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            FilesystemError::NotFound(root.to_path_buf())
        } else {
            FilesystemError::Io(error)
        }
    })?;

    if path.is_empty() {
        return Ok(root);
    }

    let requested = Path::new(path);
    let joined = if requested.is_absolute() {
        normalize_path(requested)
    } else {
        normalize_path(&root.join(requested))
    };

    if !joined.starts_with(&root) {
        return Err(FilesystemError::OutsideRoot(joined));
    }

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

fn read_prefix(path: &Path, len: usize) -> Result<Vec<u8>, FilesystemError> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut buffer = vec![0_u8; len];
    let read = file.read(&mut buffer)?;
    buffer.truncate(read);
    Ok(buffer)
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
    fn write_persists_changes() {
        let temp = tempfile::tempdir().unwrap();
        let service = FileService;
        let result = service
            .write(temp.path(), "notes.txt", "updated", DEFAULT_WRITE_LIMIT)
            .unwrap();
        assert_eq!(result.size, 7);
        let read = service
            .read(temp.path(), "notes.txt", DEFAULT_READ_LIMIT)
            .unwrap();
        assert_eq!(read.content.as_deref(), Some("updated"));
    }

    #[test]
    fn large_reads_are_truncated() {
        let temp = tempfile::tempdir().unwrap();
        let payload = "x".repeat(32);
        fs::write(temp.path().join("big.txt"), &payload).unwrap();
        let service = FileService;
        let read = service.read(temp.path(), "big.txt", 8).unwrap();
        assert!(read.truncated);
        assert_eq!(read.content.as_deref(), Some("xxxxxxxx"));
        assert_eq!(read.size, 32);
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
