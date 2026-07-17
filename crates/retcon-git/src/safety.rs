//! Default-branch protection and other Git safety checks.

use std::path::Path;

use crate::{GitError, default_branch};

/// Operations blocked on the repository default branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectedOperation {
    /// Deleting the default branch.
    BranchDelete,
    /// Force pushing to the default branch.
    ForcePush,
    /// Committing directly on the default branch.
    DirectCommit,
}

impl ProtectedOperation {
    fn message(self, branch: &str) -> String {
        match self {
            Self::BranchDelete => {
                format!("cannot delete the default branch '{branch}'")
            }
            Self::ForcePush => {
                format!("force push to the default branch '{branch}' is blocked")
            }
            Self::DirectCommit => {
                format!("direct commits to the default branch '{branch}' are blocked")
            }
        }
    }
}

/// Returns `Some(user_message)` when `branch` is protected for `operation`.
#[must_use]
pub fn check_default_branch(
    branch: &str,
    default_branch: &str,
    operation: ProtectedOperation,
) -> Option<String> {
    if branches_equal(branch, default_branch) {
        Some(operation.message(branch))
    } else {
        None
    }
}

/// Returns an error when committing directly to the default branch.
pub async fn ensure_commit_allowed(repo: &Path, branch: &str) -> Result<(), GitError> {
    let default = default_branch(repo).await?;
    if let Some(message) = check_default_branch(branch, &default, ProtectedOperation::DirectCommit)
    {
        return Err(GitError {
            command: "commit".into(),
            exit_code: None,
            stderr: message,
        });
    }
    Ok(())
}

/// Returns an error when deleting the default branch.
pub async fn ensure_branch_delete_allowed(repo: &Path, branch: &str) -> Result<(), GitError> {
    let default = default_branch(repo).await?;
    if let Some(message) = check_default_branch(branch, &default, ProtectedOperation::BranchDelete)
    {
        return Err(GitError {
            command: "branch delete".into(),
            exit_code: None,
            stderr: message,
        });
    }
    Ok(())
}

/// Returns an error when force-pushing to the default branch.
pub async fn ensure_push_allowed(repo: &Path, branch: &str, force: bool) -> Result<(), GitError> {
    if !force {
        return Ok(());
    }
    let default = default_branch(repo).await?;
    if let Some(message) = check_default_branch(branch, &default, ProtectedOperation::ForcePush) {
        return Err(GitError {
            command: "push".into(),
            exit_code: None,
            stderr: message,
        });
    }
    Ok(())
}

fn branches_equal(left: &str, right: &str) -> bool {
    normalize_branch(left) == normalize_branch(right)
}

fn normalize_branch(branch: &str) -> &str {
    branch.strip_prefix("refs/heads/").unwrap_or(branch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_default_branch_delete() {
        let msg = check_default_branch("main", "main", ProtectedOperation::BranchDelete);
        assert!(msg.is_some());
    }

    #[test]
    fn allows_feature_branch_delete() {
        let msg = check_default_branch("feature/x", "main", ProtectedOperation::BranchDelete);
        assert!(msg.is_none());
    }

    #[test]
    fn normalizes_refs_heads_prefix() {
        assert!(branches_equal("refs/heads/main", "main"));
    }
}
