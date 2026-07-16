//! Git repository operations via the native Git CLI.
//!
//! Phase 2 spike scope: open/inspect a repository, branches, worktrees, and
//! diffs, with structured failures. The full repository service is Phase 16.

use std::path::Path;

use serde::Serialize;

/// A failed Git invocation, preserving what Git actually said.
#[derive(Debug, Clone, Serialize)]
pub struct GitError {
    /// The subcommand that failed, e.g. `worktree add`.
    pub command: String,
    /// Git's exit code, if it ran at all.
    pub exit_code: Option<i32>,
    /// Git's stderr (trimmed), the human-readable explanation.
    pub stderr: String,
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "git {} failed ({:?}): {}",
            self.command, self.exit_code, self.stderr
        )
    }
}

/// Run `git` with `args` in `repo`, returning trimmed stdout.
///
/// # Errors
///
/// Returns a [`GitError`] carrying Git's exit code and stderr on failure.
pub async fn run_git(repo: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .await
        .map_err(|e| GitError {
            command: args.join(" "),
            exit_code: None,
            stderr: format!("failed to launch git: {e}"),
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_owned())
    } else {
        Err(GitError {
            command: args.join(" "),
            exit_code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

/// One entry from `git status --porcelain`.
#[derive(Debug, Clone, Serialize)]
pub struct StatusEntry {
    /// Two-character porcelain status code (e.g. ` M`, `??`).
    pub code: String,
    /// Repo-relative path.
    pub path: String,
}

/// Summary of a repository's current state.
#[derive(Debug, Clone, Serialize)]
pub struct RepoStatus {
    /// Current branch name (or `HEAD` when detached).
    pub branch: String,
    /// Changed/untracked entries from porcelain status.
    pub entries: Vec<StatusEntry>,
}

/// Read the current branch and porcelain status of `repo`.
///
/// # Errors
///
/// Returns a [`GitError`] if the directory is not a repository or Git fails.
pub async fn status(repo: &Path) -> Result<RepoStatus, GitError> {
    let branch = run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"]).await?;
    let porcelain = run_git(repo, &["status", "--porcelain"]).await?;
    let entries = porcelain
        .lines()
        .filter(|l| l.len() > 3)
        .map(|l| StatusEntry {
            code: l[..2].to_owned(),
            path: l[3..].to_owned(),
        })
        .collect();
    Ok(RepoStatus { branch, entries })
}

/// A worktree from `git worktree list --porcelain`.
#[derive(Debug, Clone, Serialize)]
pub struct Worktree {
    /// Absolute path of the worktree.
    pub path: String,
    /// Checked-out branch ref, if any.
    pub branch: Option<String>,
}

/// List the repository's worktrees.
///
/// # Errors
///
/// Returns a [`GitError`] if Git fails.
pub async fn worktree_list(repo: &Path) -> Result<Vec<Worktree>, GitError> {
    let raw = run_git(repo, &["worktree", "list", "--porcelain"]).await?;
    let mut result = Vec::new();
    let mut current: Option<Worktree> = None;
    for line in raw.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(w) = current.take() {
                result.push(w);
            }
            current = Some(Worktree {
                path: path.to_owned(),
                branch: None,
            });
        } else if let Some(branch) = line.strip_prefix("branch ")
            && let Some(w) = current.as_mut()
        {
            w.branch = Some(branch.to_owned());
        }
    }
    if let Some(w) = current {
        result.push(w);
    }
    Ok(result)
}

/// Create a worktree at `path` on a new branch `branch`.
///
/// # Errors
///
/// Returns a [`GitError`] if the branch exists or the path is unusable.
pub async fn worktree_add(repo: &Path, path: &str, branch: &str) -> Result<(), GitError> {
    run_git(repo, &["worktree", "add", "-b", branch, path])
        .await
        .map(|_| ())
}

/// Remove the worktree at `path`.
///
/// # Errors
///
/// Returns a [`GitError`] if the worktree is dirty or locked.
pub async fn worktree_remove(repo: &Path, path: &str) -> Result<(), GitError> {
    run_git(repo, &["worktree", "remove", path])
        .await
        .map(|_| ())
}

/// Produce a unified diff of uncommitted changes (optionally for one path).
///
/// # Errors
///
/// Returns a [`GitError`] if Git fails.
pub async fn diff(repo: &Path, path: Option<&str>) -> Result<String, GitError> {
    let mut args = vec!["diff"];
    if let Some(p) = path {
        args.push("--");
        args.push(p);
    }
    run_git(repo, &args).await
}
