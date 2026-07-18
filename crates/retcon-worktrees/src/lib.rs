//! Git worktree lifecycle and task/session assignment.

#![allow(missing_docs)] // Phase 16 API; public documentation lands with the generated protocol.

use std::path::Path;

use retcon_git::GitError;
use retcon_storage::{Database, GitWorktree, NewGitWorktree, StorageError};
use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

/// Worktree lifecycle errors.
#[derive(Debug, Error)]
pub enum WorktreeError {
    /// Git CLI failure.
    #[error("{0}")]
    Git(#[from] GitError),
    /// Storage failure.
    #[error("{0}")]
    Storage(#[from] StorageError),
    /// The repository path is not registered with a project.
    #[error("repository path is not registered: {0}")]
    UnknownRepository(String),
    /// A persisted worktree record was not found.
    #[error("worktree not found: {0}")]
    NotFound(String),
}

/// Manages Git worktrees and their persisted assignments.
#[derive(Clone)]
pub struct WorktreeManager {
    database: Database,
}

impl WorktreeManager {
    #[must_use]
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    /// Create a Git worktree and persist it in `git_worktrees`.
    pub async fn add(
        &self,
        repo: &Path,
        path: &str,
        branch: &str,
        session_id: Option<Uuid>,
    ) -> Result<GitWorktree, WorktreeError> {
        retcon_git::worktree_add(repo, path, branch).await?;
        let canonical = canonical_worktree_path(repo, path);
        self.persist(repo, &canonical, Some(branch), session_id)
            .await
    }

    /// Remove a Git worktree and mark the persisted record removed.
    pub async fn remove(&self, repo: &Path, path: &str) -> Result<bool, WorktreeError> {
        let canonical = canonical_worktree_path(repo, path);
        retcon_git::worktree_remove(repo, path).await?;
        let repos = &self.database;
        if let Some(record) = repos
            .git_worktrees()
            .find_by_path(&canonical)?
            .or(repos.git_worktrees().find_by_path(path)?)
        {
            repos.git_worktrees().mark_removed(record.id)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Assign a worktree path to an agent session.
    pub fn assign_session(
        &self,
        path: &str,
        session_id: Uuid,
    ) -> Result<GitWorktree, WorktreeError> {
        let repos = &self.database;
        let canonical = Path::new(path)
            .canonicalize()
            .unwrap_or_else(|_| Path::new(path).to_path_buf())
            .to_string_lossy()
            .into_owned();
        let Some(record) = repos
            .git_worktrees()
            .find_by_path(&canonical)?
            .or(repos.git_worktrees().find_by_path(path)?)
        else {
            return Err(WorktreeError::NotFound(path.to_owned()));
        };
        repos
            .git_worktrees()
            .assign_session(record.id, session_id)?;
        repos
            .git_worktrees()
            .get(record.id)?
            .ok_or_else(|| WorktreeError::NotFound(path.to_owned()))
    }

    /// List persisted worktrees for a repository root path.
    pub fn list_stored(&self, repo: &Path) -> Result<Vec<StoredWorktree>, WorktreeError> {
        let location_id = self.location_id(repo)?;
        Ok(self
            .database
            .git_worktrees()
            .list_by_repository(location_id)?
            .into_iter()
            .map(StoredWorktree::from)
            .collect())
    }

    /// List persisted worktrees assigned to a session.
    pub fn list_for_session(&self, session_id: Uuid) -> Result<Vec<StoredWorktree>, WorktreeError> {
        Ok(self
            .database
            .git_worktrees()
            .list_by_session(session_id)?
            .into_iter()
            .map(StoredWorktree::from)
            .collect())
    }

    async fn persist(
        &self,
        repo: &Path,
        path: &str,
        branch: Option<&str>,
        session_id: Option<Uuid>,
    ) -> Result<GitWorktree, WorktreeError> {
        let location_id = self.location_id(repo)?;
        let head_oid = retcon_git::head_oid(repo).await.ok();
        let repos = &self.database;
        if let Some(existing) = repos.git_worktrees().find_by_path(path)? {
            if let Some(session_id) = session_id {
                repos
                    .git_worktrees()
                    .assign_session(existing.id, session_id)?;
            }
            return repos
                .git_worktrees()
                .get(existing.id)?
                .ok_or_else(|| WorktreeError::NotFound(path.to_owned()));
        }
        let mut record = NewGitWorktree::new(location_id, path);
        record.session_id = session_id;
        record.branch = branch.map(str::to_owned);
        record.head_oid = head_oid;
        repos.git_worktrees().create(&record).map_err(Into::into)
    }

    fn location_id(&self, repo: &Path) -> Result<Uuid, WorktreeError> {
        let canonical = canonical_repo_path(repo);
        self.database
            .projects()
            .location_id_by_path(&canonical)?
            .ok_or_else(|| WorktreeError::UnknownRepository(canonical))
    }
}

fn canonical_repo_path(repo: &Path) -> String {
    repo.canonicalize()
        .unwrap_or_else(|_| repo.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn canonical_worktree_path(repo: &Path, path: &str) -> String {
    let path = Path::new(path);
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    };
    joined
        .canonicalize()
        .unwrap_or(joined)
        .to_string_lossy()
        .into_owned()
}

/// Persisted worktree record exposed to RPC clients.
#[derive(Debug, Clone, Serialize)]
pub struct StoredWorktree {
    pub id: String,
    pub session_id: Option<String>,
    pub path: String,
    pub branch: Option<String>,
    pub head_oid: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub removed_at: Option<i64>,
}

impl From<GitWorktree> for StoredWorktree {
    fn from(value: GitWorktree) -> Self {
        Self {
            id: value.id.to_string(),
            session_id: value.session_id.map(|id| id.to_string()),
            path: value.path,
            branch: value.branch,
            head_oid: value.head_oid,
            status: value.status,
            created_at: value.created_at,
            removed_at: value.removed_at,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use retcon_storage::{NewProject, NewSession, Storage};

    #[tokio::test]
    async fn persist_and_assign_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        retcon_git::run_git(&repo, &["init", "-b", "main"])
            .await
            .unwrap();
        retcon_git::run_git(&repo, &["config", "user.email", "retcon@example.invalid"])
            .await
            .unwrap();
        retcon_git::run_git(&repo, &["config", "user.name", "Retcon Test"])
            .await
            .unwrap();
        std::fs::write(repo.join("hello.txt"), "one\n").unwrap();
        retcon_git::run_git(&repo, &["add", "."]).await.unwrap();
        retcon_git::run_git(&repo, &["commit", "-m", "initial"])
            .await
            .unwrap();

        let storage = Storage::open(dir.path()).unwrap();
        let canonical = repo.canonicalize().unwrap().to_string_lossy().into_owned();
        let project = storage
            .database()
            .projects()
            .create(&NewProject::new("demo"))
            .unwrap();
        storage
            .database()
            .projects()
            .add_location(project.id, &canonical, None)
            .unwrap();
        let session = storage
            .database()
            .sessions()
            .create(&NewSession::new(project.id, "Agent"))
            .unwrap();

        let manager = WorktreeManager::new(storage.database().clone());
        let wt_path = dir.path().join("wt");
        let record = manager
            .add(
                &repo,
                wt_path.to_str().unwrap(),
                "agent/wt",
                Some(session.id),
            )
            .await
            .unwrap();
        assert_eq!(record.session_id, Some(session.id));
        assert_eq!(manager.list_stored(&repo).unwrap().len(), 1);
        manager
            .remove(&repo, wt_path.to_str().unwrap())
            .await
            .unwrap();
    }
}
