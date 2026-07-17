//! Checkpoint capture, preview, and selective restore.

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use retcon_git::DiffMode;
use retcon_storage::{
    ArtifactStore, Database, FileChange, GitCheckpoint, NewFileChange, NewGitCheckpoint,
    NewGitWorktree,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::CheckpointError;
use crate::kind::CheckpointKind;

/// Handle returned from [`CheckpointService::begin_file_write`] to finalize the snapshot.
#[derive(Debug, Clone)]
pub struct FileWriteCheckpoint {
    pub checkpoint_id: Uuid,
    pub turn_id: Option<Uuid>,
    pub relative_path: String,
    before_artifact_hash: Option<String>,
}

/// One path in a rollback preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackPreviewItem {
    pub path: String,
    pub change_kind: String,
    pub action: String,
    pub conflict: bool,
    pub before_artifact_hash: Option<String>,
    pub after_artifact_hash: Option<String>,
}

/// What would happen if a checkpoint were restored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackPreview {
    pub checkpoint_id: String,
    pub kind: String,
    pub items: Vec<RollbackPreviewItem>,
}

/// Outcome of restoring selected paths from a checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub checkpoint_id: String,
    pub restored: Vec<String>,
    pub skipped: Vec<String>,
    pub conflicts: Vec<String>,
}

/// Checkpoint capture and rollback orchestration.
#[derive(Clone)]
pub struct CheckpointService {
    database: Database,
    artifacts: ArtifactStore,
}

impl CheckpointService {
    #[must_use]
    pub fn new(database: Database, artifacts: ArtifactStore) -> Self {
        Self {
            database,
            artifacts,
        }
    }

    /// Create a manual checkpoint for a repository root.
    pub async fn create_manual(
        &self,
        root: &Path,
        turn_id: Option<Uuid>,
    ) -> Result<GitCheckpoint, CheckpointError> {
        self.create_git_snapshot(root, CheckpointKind::Manual, turn_id)
            .await
    }

    /// Create a turn-start checkpoint for a repository root.
    pub async fn create_turn_start(
        &self,
        root: &Path,
        turn_id: Uuid,
    ) -> Result<GitCheckpoint, CheckpointError> {
        self.create_git_snapshot(root, CheckpointKind::TurnStart, Some(turn_id))
            .await
    }

    /// Capture state before a mutating Git RPC runs.
    pub async fn before_git_operation(
        &self,
        repo: &Path,
        turn_id: Option<Uuid>,
    ) -> Result<GitCheckpoint, CheckpointError> {
        self.create_git_snapshot(repo, CheckpointKind::PreGit, turn_id)
            .await
    }

    /// Begin a pre-file-write checkpoint for a relative path under `root`.
    pub fn begin_file_write(
        &self,
        root: &Path,
        relative_path: &str,
        turn_id: Option<Uuid>,
    ) -> Result<FileWriteCheckpoint, CheckpointError> {
        let worktree_id = self.resolve_worktree(root)?;
        let absolute = resolve_within_root(root, relative_path)?;
        let before_artifact_hash = if absolute.is_file() {
            Some(self.store_file_bytes(&fs::read(&absolute)?)?)
        } else {
            None
        };

        let mut checkpoint =
            NewGitCheckpoint::new(worktree_id, CheckpointKind::PreFileWrite.as_str());
        checkpoint.turn_id = turn_id;
        let checkpoint = self.database.git_checkpoints().create(&checkpoint)?;

        Ok(FileWriteCheckpoint {
            checkpoint_id: checkpoint.id,
            turn_id,
            relative_path: relative_path.to_owned(),
            before_artifact_hash,
        })
    }

    /// Finalize a file-write checkpoint after the write succeeds.
    pub fn finish_file_write(
        &self,
        root: &Path,
        snapshot: FileWriteCheckpoint,
    ) -> Result<FileChange, CheckpointError> {
        let absolute = resolve_within_root(root, &snapshot.relative_path)?;
        let after_artifact_hash = if absolute.is_file() {
            Some(self.store_file_bytes(&fs::read(&absolute)?)?)
        } else {
            None
        };

        let change_kind = match (&snapshot.before_artifact_hash, &after_artifact_hash) {
            (None, Some(_)) => "create",
            (Some(_), None) => "delete",
            (Some(_), Some(_)) => "modify",
            (None, None) => "noop",
        };

        let mut change = NewFileChange::new(&snapshot.relative_path, change_kind);
        change.turn_id = snapshot.turn_id;
        change.git_checkpoint_id = Some(snapshot.checkpoint_id);
        change.before_artifact_hash = snapshot.before_artifact_hash;
        change.after_artifact_hash = after_artifact_hash;
        self.database.file_changes().create(&change).map_err(Into::into)
    }

    pub fn get(&self, checkpoint_id: Uuid) -> Result<Option<GitCheckpoint>, CheckpointError> {
        Ok(self.database.git_checkpoints().get(checkpoint_id)?)
    }

    pub fn list_for_root(
        &self,
        root: &Path,
        limit: usize,
    ) -> Result<Vec<GitCheckpoint>, CheckpointError> {
        let worktree_id = self.resolve_worktree(root)?;
        Ok(self
            .database
            .git_checkpoints()
            .list_by_worktree(worktree_id, limit)?)
    }

    pub fn list_for_turn(&self, turn_id: Uuid) -> Result<Vec<GitCheckpoint>, CheckpointError> {
        Ok(self.database.git_checkpoints().list_for_turn(turn_id)?)
    }

    pub fn file_changes_for_checkpoint(
        &self,
        checkpoint_id: Uuid,
    ) -> Result<Vec<FileChange>, CheckpointError> {
        Ok(self
            .database
            .file_changes()
            .list_for_checkpoint(checkpoint_id)?)
    }

    pub fn preview_rollback(
        &self,
        root: &Path,
        checkpoint_id: Uuid,
    ) -> Result<RollbackPreview, CheckpointError> {
        let checkpoint = self
            .database
            .git_checkpoints()
            .get(checkpoint_id)?
            .ok_or_else(|| CheckpointError::NotFound(checkpoint_id.to_string()))?;
        let changes = self.file_changes_for_checkpoint(checkpoint_id)?;
        let mut items = Vec::new();
        for change in changes {
            items.push(self.preview_item(root, &change)?);
        }
        Ok(RollbackPreview {
            checkpoint_id: checkpoint_id.to_string(),
            kind: checkpoint.kind,
            items,
        })
    }

    pub fn restore_selective(
        &self,
        root: &Path,
        checkpoint_id: Uuid,
        paths: &[String],
        force: bool,
    ) -> Result<RestoreReport, CheckpointError> {
        let preview = self.preview_rollback(root, checkpoint_id)?;
        let selected: std::collections::HashSet<&str> =
            paths.iter().map(String::as_str).collect();
        let mut restored = Vec::new();
        let mut skipped = Vec::new();
        let conflicts: Vec<String> = preview
            .items
            .iter()
            .filter(|item| selected.contains(item.path.as_str()) && item.conflict)
            .map(|item| item.path.clone())
            .collect();

        // Preflight the complete selection before touching the worktree. Otherwise a
        // later conflict can leave earlier paths restored even though this method
        // reports failure to the caller.
        if !force && !conflicts.is_empty() {
            return Err(CheckpointError::Conflicts(conflicts.join(", ")));
        }

        for item in preview.items {
            if !selected.contains(item.path.as_str()) {
                continue;
            }
            if self.restore_item(root, &item)? {
                restored.push(item.path);
            } else {
                skipped.push(item.path);
            }
        }

        Ok(RestoreReport {
            checkpoint_id: checkpoint_id.to_string(),
            restored,
            skipped,
            conflicts: if force { conflicts } else { Vec::new() },
        })
    }

    async fn create_git_snapshot(
        &self,
        root: &Path,
        kind: CheckpointKind,
        turn_id: Option<Uuid>,
    ) -> Result<GitCheckpoint, CheckpointError> {
        let worktree_id = self.resolve_worktree(root)?;
        let base_oid = retcon_git::head_oid(root).await.ok();
        let patch = retcon_git::diff(root, None, DiffMode::All)
            .await
            .unwrap_or_default();
        let patch_artifact_hash = if patch.is_empty() {
            None
        } else {
            Some(self.store_file_bytes(patch.as_bytes())?)
        };

        let mut checkpoint = NewGitCheckpoint::new(worktree_id, kind.as_str());
        checkpoint.turn_id = turn_id;
        checkpoint.base_oid = base_oid;
        checkpoint.patch_artifact_hash = patch_artifact_hash;
        self.database.git_checkpoints().create(&checkpoint).map_err(Into::into)
    }

    fn resolve_worktree(&self, root: &Path) -> Result<Uuid, CheckpointError> {
        let path = canonical_path(root);
        if let Some(worktree) = self.database.git_worktrees().find_by_path(&path)? {
            return Ok(worktree.id);
        }
        let Some(location_id) = self.database.projects().location_id_by_path(&path)? else {
            return Err(CheckpointError::UnknownRepository(path));
        };
        let record = NewGitWorktree::new(location_id, &path);
        Ok(self.database.git_worktrees().create(&record)?.id)
    }

    fn store_file_bytes(&self, bytes: &[u8]) -> Result<String, CheckpointError> {
        Ok(self.artifacts.store_bytes(bytes)?.hash)
    }

    fn read_artifact_bytes(&self, hash: &str) -> Result<Vec<u8>, CheckpointError> {
        let mut file = self.artifacts.get(hash)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn preview_item(
        &self,
        root: &Path,
        change: &FileChange,
    ) -> Result<RollbackPreviewItem, CheckpointError> {
        let absolute = resolve_within_root(root, &change.path)?;
        let current_hash = if absolute.is_file() {
            Some(sha256_hex(&fs::read(&absolute)?))
        } else {
            None
        };
        let conflict = match (
            current_hash.as_deref(),
            change.before_artifact_hash.as_deref(),
            change.after_artifact_hash.as_deref(),
        ) {
            (Some(current), Some(before), Some(after)) => current != before && current != after,
            (Some(current), None, Some(after)) => current != after,
            (Some(current), Some(before), None) => current != before,
            _ => false,
        };
        let action = match (&change.before_artifact_hash, change.after_artifact_hash.as_deref()) {
            (None, Some(_)) => "delete",
            (Some(_), _) => "restore",
            (None, None) => "skip",
        };
        Ok(RollbackPreviewItem {
            path: change.path.clone(),
            change_kind: change.change_kind.clone(),
            action: action.into(),
            conflict,
            before_artifact_hash: change.before_artifact_hash.clone(),
            after_artifact_hash: change.after_artifact_hash.clone(),
        })
    }

    fn restore_item(
        &self,
        root: &Path,
        item: &RollbackPreviewItem,
    ) -> Result<bool, CheckpointError> {
        let absolute = resolve_within_root(root, &item.path)?;
        match item.action.as_str() {
            "delete" => {
                if absolute.exists() {
                    if absolute.is_dir() {
                        fs::remove_dir_all(&absolute)?;
                    } else {
                        fs::remove_file(&absolute)?;
                    }
                }
                Ok(true)
            }
            "restore" => {
                let Some(hash) = item.before_artifact_hash.as_deref() else {
                    return Ok(false);
                };
                let bytes = self.read_artifact_bytes(hash)?;
                if let Some(parent) = absolute.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&absolute, bytes)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

fn canonical_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn resolve_within_root(root: &Path, relative_path: &str) -> Result<PathBuf, CheckpointError> {
    let root = fs::canonicalize(root).map_err(|error| {
        CheckpointError::InvalidRequest(format!("invalid project root: {error}"))
    })?;
    let requested = Path::new(relative_path);
    if !requested.is_absolute()
        && requested
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
    {
        return Err(CheckpointError::OutsideRoot(relative_path.to_owned()));
    }

    let joined = if relative_path.is_empty() {
        root.clone()
    } else if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        root.join(requested)
    };

    // Canonicalize the nearest existing ancestor as well as existing targets.
    // This catches both symlink escapes and not-yet-created paths outside the root.
    let existing_ancestor = joined
        .ancestors()
        .find(|ancestor| ancestor.exists())
        .ok_or_else(|| CheckpointError::OutsideRoot(joined.to_string_lossy().into_owned()))?;
    let canonical_ancestor = existing_ancestor.canonicalize()?;
    if !canonical_ancestor.starts_with(&root) {
        return Err(CheckpointError::OutsideRoot(
            joined.to_string_lossy().into_owned(),
        ));
    }
    if joined.exists() {
        let canonical = joined.canonicalize()?;
        if !canonical.starts_with(&root) {
            return Err(CheckpointError::OutsideRoot(
                canonical.to_string_lossy().into_owned(),
            ));
        }
        Ok(canonical)
    } else {
        Ok(joined)
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use retcon_storage::{NewProject, Storage};

    fn setup() -> (tempfile::TempDir, CheckpointService, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir_all(&root).unwrap();
        let storage = Storage::open(temp.path()).unwrap();
        let canonical = root.canonicalize().unwrap().to_string_lossy().into_owned();
        let project = NewProject::new("demo");
        storage.database().projects().create(&project).unwrap();
        storage
            .database()
            .projects()
            .add_location(project.id, &canonical, None)
            .unwrap();
        let service = CheckpointService::new(
            storage.database().clone(),
            storage.artifacts().clone(),
        );
        (temp, service, root)
    }

    #[test]
    fn file_write_checkpoint_round_trip() {
        let (_temp, service, root) = setup();
        fs::write(root.join("note.txt"), "before").unwrap();
        let snapshot = service
            .begin_file_write(&root, "note.txt", None)
            .unwrap();
        fs::write(root.join("note.txt"), "after").unwrap();
        service.finish_file_write(&root, snapshot).unwrap();
        let checkpoints = service.list_for_root(&root, 10).unwrap();
        assert_eq!(checkpoints.len(), 1);
        let changes = service
            .file_changes_for_checkpoint(checkpoints[0].id)
            .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_kind, "modify");
    }

    #[test]
    fn selective_restore_reverts_modified_file() {
        let (_temp, service, root) = setup();
        fs::write(root.join("note.txt"), "before").unwrap();
        let snapshot = service
            .begin_file_write(&root, "note.txt", None)
            .unwrap();
        fs::write(root.join("note.txt"), "after").unwrap();
        let change = service.finish_file_write(&root, snapshot).unwrap();
        let report = service
            .restore_selective(
                &root,
                change.git_checkpoint_id.unwrap(),
                &[change.path.clone()],
                false,
            )
            .unwrap();
        assert_eq!(report.restored, vec!["note.txt"]);
        assert_eq!(fs::read_to_string(root.join("note.txt")).unwrap(), "before");
    }

    #[test]
    fn unrelated_user_edits_block_restore_without_force() {
        let (_temp, service, root) = setup();
        fs::write(root.join("note.txt"), "before").unwrap();
        let snapshot = service
            .begin_file_write(&root, "note.txt", None)
            .unwrap();
        fs::write(root.join("note.txt"), "after").unwrap();
        let change = service.finish_file_write(&root, snapshot).unwrap();
        fs::write(root.join("note.txt"), "user-edit").unwrap();
        let error = service
            .restore_selective(
                &root,
                change.git_checkpoint_id.unwrap(),
                &[change.path.clone()],
                false,
            )
            .unwrap_err();
        assert!(matches!(error, CheckpointError::Conflicts(_)));
    }

    #[test]
    fn conflict_preflight_prevents_partial_restore() {
        let (_temp, service, root) = setup();
        fs::write(root.join("a.txt"), "before-a").unwrap();
        let snapshot = service.begin_file_write(&root, "a.txt", None).unwrap();
        fs::write(root.join("a.txt"), "after-a").unwrap();
        let first = service.finish_file_write(&root, snapshot).unwrap();
        let checkpoint_id = first.git_checkpoint_id.unwrap();

        std::thread::sleep(std::time::Duration::from_millis(2));
        let before_hash = service.store_file_bytes(b"before-b").unwrap();
        let after_hash = service.store_file_bytes(b"after-b").unwrap();
        let mut second = NewFileChange::new("b.txt", "modify");
        second.git_checkpoint_id = Some(checkpoint_id);
        second.before_artifact_hash = Some(before_hash);
        second.after_artifact_hash = Some(after_hash);
        service.database.file_changes().create(&second).unwrap();
        fs::write(root.join("b.txt"), "user-edit-b").unwrap();

        let error = service
            .restore_selective(
                &root,
                checkpoint_id,
                &["a.txt".into(), "b.txt".into()],
                false,
            )
            .unwrap_err();

        assert!(matches!(error, CheckpointError::Conflicts(_)));
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "after-a");
        assert_eq!(fs::read_to_string(root.join("b.txt")).unwrap(), "user-edit-b");
    }

    #[test]
    fn nonexistent_parent_traversal_is_rejected() {
        let (temp, service, root) = setup();
        let error = service
            .begin_file_write(&root, "../outside/new.txt", None)
            .unwrap_err();

        assert!(matches!(error, CheckpointError::OutsideRoot(_)));
        assert!(!temp.path().join("outside/new.txt").exists());
        assert!(service.list_for_root(&root, 10).unwrap().is_empty());
    }

    #[test]
    fn force_restore_reports_overwritten_conflicts() {
        let (_temp, service, root) = setup();
        fs::write(root.join("note.txt"), "before").unwrap();
        let snapshot = service.begin_file_write(&root, "note.txt", None).unwrap();
        fs::write(root.join("note.txt"), "after").unwrap();
        let change = service.finish_file_write(&root, snapshot).unwrap();
        fs::write(root.join("note.txt"), "user-edit").unwrap();

        let report = service
            .restore_selective(
                &root,
                change.git_checkpoint_id.unwrap(),
                &[change.path],
                true,
            )
            .unwrap();

        assert_eq!(report.restored, vec!["note.txt"]);
        assert_eq!(report.conflicts, vec!["note.txt"]);
        assert_eq!(fs::read_to_string(root.join("note.txt")).unwrap(), "before");
    }
}
