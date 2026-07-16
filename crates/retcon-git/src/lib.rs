//! Git repository operations via the native Git CLI.
//!
//! Phase 2 spike scope: open/inspect a repository, branches, worktrees, and
//! diffs, with structured failures. The full repository service is Phase 16.

use std::path::Path;

use serde::Serialize;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Maximum stdout captured from a single Git invocation.
pub const MAX_GIT_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

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
    run_git_limited(repo, args, MAX_GIT_OUTPUT_BYTES).await
}

async fn run_git_limited(
    repo: &Path,
    args: &[&str],
    max_bytes: usize,
) -> Result<String, GitError> {
    let command_label = args.join(" ");
    let mut child = Command::new("git")
        .args(args)
        .current_dir(repo)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| GitError {
            command: command_label.clone(),
            exit_code: None,
            stderr: format!("failed to launch git: {e}"),
        })?;

    let mut stdout = child.stdout.take().ok_or_else(|| GitError {
        command: command_label.clone(),
        exit_code: None,
        stderr: "git process has no stdout".into(),
    })?;
    let mut stderr = child.stderr.take().ok_or_else(|| GitError {
        command: command_label.clone(),
        exit_code: None,
        stderr: "git process has no stderr".into(),
    })?;

    let stdout_task = tokio::spawn(async move {
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 8192];
        loop {
            let read = stdout
                .read(&mut chunk)
                .await
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            if buffer.len().saturating_add(read) > max_bytes {
                return Err(format!("git output exceeded {max_bytes} bytes"));
            }
            buffer.extend_from_slice(&chunk[..read]);
        }
        Ok(buffer)
    });

    let stderr_task = tokio::spawn(async move {
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let read = stderr
                .read(&mut chunk)
                .await
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            if buffer.len().saturating_add(read) > max_bytes {
                return Err(format!("git stderr exceeded {max_bytes} bytes"));
            }
            buffer.extend_from_slice(&chunk[..read]);
        }
        Ok(buffer)
    });

    let status = child.wait().await.map_err(|e| GitError {
        command: command_label.clone(),
        exit_code: None,
        stderr: format!("failed to wait for git: {e}"),
    })?;

    let stdout_bytes = stdout_task
        .await
        .map_err(|e| GitError {
            command: command_label.clone(),
            exit_code: status.code(),
            stderr: format!("stdout reader failed: {e}"),
        })?
        .map_err(|message| GitError {
            command: command_label.clone(),
            exit_code: status.code(),
            stderr: message,
        })?;
    let stderr_bytes = stderr_task
        .await
        .map_err(|e| GitError {
            command: command_label.clone(),
            exit_code: status.code(),
            stderr: format!("stderr reader failed: {e}"),
        })?
        .map_err(|message| GitError {
            command: command_label.clone(),
            exit_code: status.code(),
            stderr: message,
        })?;

    if status.success() {
        Ok(String::from_utf8_lossy(&stdout_bytes)
            .trim_end()
            .to_owned())
    } else {
        Err(GitError {
            command: command_label,
            exit_code: status.code(),
            stderr: String::from_utf8_lossy(&stderr_bytes).trim().to_owned(),
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

/// Read the current branch and porcelain status of `repo` in one Git invocation.
///
/// # Errors
///
/// Returns a [`GitError`] if the directory is not a repository or Git fails.
pub async fn status(repo: &Path) -> Result<RepoStatus, GitError> {
    let raw = run_git(repo, &["status", "--porcelain=v1", "--branch"]).await?;
    let mut lines = raw.lines();
    let branch = lines
        .next()
        .and_then(|line| line.strip_prefix("## "))
        .map(parse_branch_header)
        .unwrap_or_else(|| "HEAD".to_owned());
    let entries = lines
        .filter(|line| line.len() > 3)
        .map(|line| StatusEntry {
            code: line[..2].to_owned(),
            path: line[3..].to_owned(),
        })
        .collect();
    Ok(RepoStatus { branch, entries })
}

fn parse_branch_header(header: &str) -> String {
    header
        .split("...")
        .next()
        .unwrap_or(header)
        .trim_start_matches('#')
        .trim()
        .to_owned()
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

/// Create a branch without checking it out.
pub async fn branch_create(repo: &Path, branch: &str) -> Result<(), GitError> {
    run_git(repo, &["branch", branch]).await.map(|_| ())
}

/// Return paths currently reported as unmerged conflicts.
pub async fn conflicts(repo: &Path) -> Result<Vec<String>, GitError> {
    let raw = run_git(repo, &["diff", "--name-only", "--diff-filter=U"]).await?;
    Ok(raw.lines().map(str::to_owned).collect())
}

/// Return configured submodule paths, without recursively executing their config.
pub async fn submodules(repo: &Path) -> Result<Vec<String>, GitError> {
    let file = repo.join(".gitmodules");
    if !file.exists() {
        return Ok(Vec::new());
    }
    let raw = run_git(
        repo,
        &["config", "--file", ".gitmodules", "--get-regexp", "path"],
    )
    .await?;
    Ok(raw
        .lines()
        .filter_map(|line| line.split_once(' ').map(|(_, path)| path.to_owned()))
        .collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn status_branch_diff_and_worktree_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        run_git(dir.path(), &["init", "-b", "main"]).await.unwrap();
        run_git(
            dir.path(),
            &["config", "user.email", "retcon@example.invalid"],
        )
        .await
        .unwrap();
        run_git(dir.path(), &["config", "user.name", "Retcon Test"])
            .await
            .unwrap();
        std::fs::write(dir.path().join("hello.txt"), "one\n").unwrap();
        run_git(dir.path(), &["add", "."]).await.unwrap();
        run_git(dir.path(), &["commit", "-m", "initial"])
            .await
            .unwrap();
        branch_create(dir.path(), "review").await.unwrap();
        std::fs::write(dir.path().join("hello.txt"), "two\n").unwrap();
        assert_eq!(status(dir.path()).await.unwrap().branch, "main");
        assert!(diff(dir.path(), None).await.unwrap().contains("+two"));
        let worktree = dir.path().join("worktree");
        worktree_add(dir.path(), worktree.to_str().unwrap(), "spike")
            .await
            .unwrap();
        assert!(worktree_list(dir.path()).await.unwrap().len() >= 2);
        worktree_remove(dir.path(), worktree.to_str().unwrap())
            .await
            .unwrap();
        assert!(conflicts(dir.path()).await.unwrap().is_empty());
    }
}
