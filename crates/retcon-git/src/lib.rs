//! Git repository operations via the native Git CLI.
//!
//! Phase 16 scope: branch workflow, commit/stage, diffs, worktrees, and default
//! branch protection.

mod safety;

use std::path::Path;
use std::time::Instant;

pub use safety::{
    ProtectedOperation, check_default_branch, ensure_branch_delete_allowed, ensure_commit_allowed,
    ensure_push_allowed,
};

use serde::Serialize;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Maximum stdout captured from a single Git invocation.
pub const MAX_GIT_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

fn runtime_metrics_enabled() -> bool {
    retcon_runtime_observability::is_enabled()
}

struct GitMetric {
    operation: &'static str,
    started: Instant,
    outcome: &'static str,
}

impl GitMetric {
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

impl Drop for GitMetric {
    fn drop(&mut self) {
        retcon_runtime_observability::record_duration(
            "git",
            "git.operation.duration",
            self.operation,
            self.outcome,
            self.started.elapsed(),
        );
        if !runtime_metrics_enabled() {
            return;
        }
        tracing::info!(
            target: "retcon_runtime",
            event = "git.operation.completed",
            component = "git",
            operation = self.operation,
            outcome = self.outcome,
            duration_ms = self.started.elapsed().as_millis() as u64
        );
    }
}

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
impl std::error::Error for GitError {}

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

async fn run_git_limited(repo: &Path, args: &[&str], max_bytes: usize) -> Result<String, GitError> {
    let operation = git_operation(args);
    let mut metric = GitMetric::new(operation);
    let command_label = args.join(" ");
    let mut child = Command::new("git")
        .args(args)
        .current_dir(repo)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
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
        read_capped_stream(&mut stdout, max_bytes).await
    });
    let stderr_task = tokio::spawn(async move {
        read_capped_stream(&mut stderr, max_bytes).await
    });

    let (stdout_join, stderr_join) = tokio::join!(stdout_task, stderr_task);
    let stdout_result = stdout_join.map_err(|e| GitError {
        command: command_label.clone(),
        exit_code: None,
        stderr: format!("stdout reader failed: {e}"),
    })?;
    let stderr_result = stderr_join.map_err(|e| GitError {
        command: command_label.clone(),
        exit_code: None,
        stderr: format!("stderr reader failed: {e}"),
    })?;
    if stdout_result.is_err() || stderr_result.is_err() {
        let _ = child.kill().await;
    }

    let status = child.wait().await.map_err(|e| GitError {
        command: command_label.clone(),
        exit_code: None,
        stderr: format!("failed to wait for git: {e}"),
    })?;

    let stdout_bytes = stdout_result.map_err(|message| GitError {
        command: command_label.clone(),
        exit_code: status.code(),
        stderr: message,
    })?;
    let stderr_bytes = stderr_result.map_err(|message| GitError {
        command: command_label.clone(),
        exit_code: status.code(),
        stderr: message,
    })?;

    if status.success() {
        metric.succeed();
        Ok(String::from_utf8_lossy(&stdout_bytes).trim_end().to_owned())
    } else {
        Err(GitError {
            command: command_label,
            exit_code: status.code(),
            stderr: String::from_utf8_lossy(&stderr_bytes).trim().to_owned(),
        })
    }
}

fn git_operation(args: &[&str]) -> &'static str {
    match args.first().copied().unwrap_or_default() {
        "add" | "apply" | "restore" => "stage",
        "branch" => "branch",
        "checkout" | "switch" => "checkout",
        "commit" => "commit",
        "config" | "symbolic-ref" | "rev-parse" => "metadata",
        "diff" => "diff",
        "init" => "init",
        "push" => "push",
        "status" => "status",
        "worktree" => "worktree",
        _ => "other",
    }
}

async fn read_capped_stream(
    stream: &mut (impl AsyncReadExt + Unpin),
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    let mut overflowed = false;
    loop {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        if overflowed {
            continue;
        }
        if buffer.len().saturating_add(read) > max_bytes {
            overflowed = true;
            continue;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    if overflowed {
        return Err(format!("git output exceeded {max_bytes} bytes"));
    }
    Ok(buffer)
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
    Ok(parse_status_output(&raw))
}

/// Parse `git status --porcelain=v1 --branch` stdout into a [`RepoStatus`].
#[must_use]
pub fn parse_status_output(raw: &str) -> RepoStatus {
    let mut lines = raw.lines();
    let branch = lines
        .next()
        .and_then(|line| line.strip_prefix("## "))
        .map(parse_branch_header)
        .unwrap_or_else(|| "HEAD".to_owned());
    let entries = lines.filter_map(parse_porcelain_line).collect();
    RepoStatus { branch, entries }
}

fn parse_porcelain_line(line: &str) -> Option<StatusEntry> {
    if line.len() < 4 {
        return None;
    }
    let code = line[..2].to_owned();
    let path = parse_porcelain_path(&line[3..]);
    if path.is_empty() {
        return None;
    }
    Some(StatusEntry { code, path })
}

/// Normalize a porcelain path field (handles renames and trailing slashes).
fn parse_porcelain_path(rest: &str) -> String {
    let path = rest
        .split_once(" -> ")
        .map(|(_, new)| new)
        .unwrap_or(rest)
        .trim()
        .trim_matches('"');
    path.trim_end_matches('/').replace('\\', "/")
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

/// Diff scope for [`diff`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffMode {
    /// Working tree vs index.
    #[default]
    Unstaged,
    /// Index vs HEAD.
    Staged,
    /// Working tree vs HEAD.
    All,
}

/// Produce a unified diff (optionally for one path).
///
/// # Errors
///
/// Returns a [`GitError`] if Git fails.
pub async fn diff(repo: &Path, path: Option<&str>, mode: DiffMode) -> Result<String, GitError> {
    let mut args = match mode {
        DiffMode::Unstaged => vec!["diff"],
        DiffMode::Staged => vec!["diff", "--cached"],
        DiffMode::All => vec!["diff", "HEAD"],
    };
    if let Some(p) = path {
        args.push("--");
        args.push(p);
    }
    run_git(repo, &args).await
}

/// List local branch names.
pub async fn branch_list(repo: &Path) -> Result<Vec<String>, GitError> {
    let raw = run_git(repo, &["branch", "--format=%(refname:short)"]).await?;
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Resolve the repository default branch name.
pub async fn default_branch(repo: &Path) -> Result<String, GitError> {
    if let Ok(raw) = run_git(
        repo,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .await
    {
        return Ok(raw.strip_prefix("origin/").unwrap_or(&raw).to_owned());
    }
    for candidate in ["main", "master"] {
        if run_git(repo, &["rev-parse", "--verify", candidate])
            .await
            .is_ok()
        {
            return Ok(candidate.to_owned());
        }
    }
    Ok("main".to_owned())
}

/// Current HEAD object id.
pub async fn head_oid(repo: &Path) -> Result<String, GitError> {
    run_git(repo, &["rev-parse", "HEAD"]).await
}

/// Create a branch without checking it out.
pub async fn branch_create(repo: &Path, branch: &str) -> Result<(), GitError> {
    run_git(repo, &["branch", branch]).await.map(|_| ())
}

/// Delete a local branch.
pub async fn branch_delete(repo: &Path, branch: &str, force: bool) -> Result<(), GitError> {
    safety::ensure_branch_delete_allowed(repo, branch).await?;
    let flag = if force { "-D" } else { "-d" };
    run_git(repo, &["branch", flag, branch]).await.map(|_| ())
}

/// Check out a branch.
pub async fn checkout(repo: &Path, branch: &str) -> Result<(), GitError> {
    run_git(repo, &["checkout", branch]).await.map(|_| ())
}

/// Stage path(s). Pass `None` to stage all changes.
pub async fn stage(repo: &Path, path: Option<&str>) -> Result<(), GitError> {
    let mut args = vec!["add"];
    if let Some(p) = path {
        args.push("--");
        args.push(p);
    } else {
        args.push("-A");
    }
    run_git(repo, &args).await.map(|_| ())
}

/// Unstage path(s). Pass `None` to unstage all changes.
pub async fn unstage(repo: &Path, path: Option<&str>) -> Result<(), GitError> {
    let mut args = vec!["restore", "--staged"];
    if let Some(p) = path {
        args.push("--");
        args.push(p);
    } else {
        args.push(".");
    }
    run_git(repo, &args).await.map(|_| ())
}

/// Stage a unified-diff hunk via `git apply --cached`.
pub async fn stage_hunk(repo: &Path, patch: &str) -> Result<(), GitError> {
    apply_patch(repo, patch, &["apply", "--cached"]).await
}

/// Discard a working-tree hunk via reverse apply.
pub async fn discard_hunk(repo: &Path, patch: &str) -> Result<(), GitError> {
    apply_patch(repo, patch, &["apply", "-R"]).await
}

async fn apply_patch(repo: &Path, patch: &str, prefix: &[&str]) -> Result<(), GitError> {
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    if patch.len() > MAX_GIT_OUTPUT_BYTES {
        return Err(GitError {
            command: prefix.join(" "),
            exit_code: None,
            stderr: format!("patch exceeds {MAX_GIT_OUTPUT_BYTES} bytes"),
        });
    }

    let mut args: Vec<&str> = prefix.to_vec();
    args.push("-");
    let command_label = args.join(" ");

    let mut child = Command::new("git")
        .args(&args)
        .current_dir(repo)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| GitError {
            command: command_label.clone(),
            exit_code: None,
            stderr: format!("failed to launch git: {e}"),
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(patch.as_bytes())
            .await
            .map_err(|e| GitError {
                command: command_label.clone(),
                exit_code: None,
                stderr: format!("failed to write patch: {e}"),
            })?;
        drop(stdin);
    }

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
        read_capped_stream(&mut stdout, MAX_GIT_OUTPUT_BYTES).await
    });
    let stderr_task = tokio::spawn(async move {
        read_capped_stream(&mut stderr, MAX_GIT_OUTPUT_BYTES).await
    });
    let (stdout_join, stderr_join) = tokio::join!(stdout_task, stderr_task);
    let stdout_result = stdout_join.map_err(|e| GitError {
        command: command_label.clone(),
        exit_code: None,
        stderr: format!("stdout reader failed: {e}"),
    })?;
    let stderr_result = stderr_join.map_err(|e| GitError {
        command: command_label.clone(),
        exit_code: None,
        stderr: format!("stderr reader failed: {e}"),
    })?;
    if stdout_result.is_err() || stderr_result.is_err() {
        let _ = child.kill().await;
    }

    let status = child.wait().await.map_err(|e| GitError {
        command: command_label.clone(),
        exit_code: None,
        stderr: format!("failed to wait for git: {e}"),
    })?;

    let _ = stdout_result.map_err(|message| GitError {
        command: command_label.clone(),
        exit_code: status.code(),
        stderr: message,
    })?;
    let stderr_bytes = stderr_result.map_err(|message| GitError {
        command: command_label.clone(),
        exit_code: status.code(),
        stderr: message,
    })?;

    if status.success() {
        Ok(())
    } else {
        Err(GitError {
            command: command_label,
            exit_code: status.code(),
            stderr: String::from_utf8_lossy(&stderr_bytes).trim().to_owned(),
        })
    }
}

/// Create a commit with the given message.
pub async fn commit(repo: &Path, message: &str, allow_empty: bool) -> Result<String, GitError> {
    let branch = status(repo).await?.branch;
    safety::ensure_commit_allowed(repo, &branch).await?;
    let mut args = vec!["commit", "-m", message];
    if allow_empty {
        args.push("--allow-empty");
    }
    run_git(repo, &args).await?;
    head_oid(repo).await
}

/// Push the current or named branch to a remote.
pub async fn push(
    repo: &Path,
    remote: &str,
    branch: Option<&str>,
    force: bool,
) -> Result<(), GitError> {
    if force {
        let target = match branch {
            Some(name) => name.to_owned(),
            None => status(repo).await?.branch,
        };
        safety::ensure_push_allowed(repo, &target, true).await?;
    }
    let mut args = vec!["push", remote];
    if force {
        args.push("--force-with-lease");
    }
    if let Some(branch) = branch {
        args.push(branch);
    }
    run_git(repo, &args).await.map(|_| ())
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

    #[test]
    fn parse_status_output_handles_branch_and_renames() {
        let status = parse_status_output(
            "## main...origin/main\n M src/a.rs\n?? notes.txt\nR  old.txt -> new.txt\n",
        );
        assert_eq!(status.branch, "main");
        assert_eq!(status.entries.len(), 3);
        assert_eq!(status.entries[0].code, " M");
        assert_eq!(status.entries[0].path, "src/a.rs");
        assert_eq!(status.entries[1].path, "notes.txt");
        assert_eq!(status.entries[2].code, "R ");
        assert_eq!(status.entries[2].path, "new.txt");
    }

    #[test]
    fn telemetry_git_operation_never_contains_arguments() {
        assert_eq!(git_operation(&["commit", "-m", "secret"]), "commit");
        assert_eq!(
            git_operation(&["unknown", "https://user:pass@example.invalid"]),
            "other"
        );
    }

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
        assert!(
            diff(dir.path(), None, DiffMode::Unstaged)
                .await
                .unwrap()
                .contains("+two")
        );
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
