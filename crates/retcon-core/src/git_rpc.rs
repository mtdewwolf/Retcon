//! RPC adapter for Git operations (Phase 16).

use std::path::{Path, PathBuf};

use retcon_git::{DiffMode, GitError};
use retcon_worktrees::{WorktreeError, WorktreeManager};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::{Request, Response};
use crate::state::CoreState;

fn failed(id: u64, error: CoreError) -> Response {
    Response::error(id, &error)
}

fn git_fail(id: u64, error: GitError) -> Response {
    failed(
        id,
        CoreError::new(
            ErrorCode::Io,
            ErrorSource::Rpc,
            "The Git operation failed.",
            format!(
                "git {} (exit {:?}): {}",
                error.command, error.exit_code, error.stderr
            ),
        ),
    )
}

fn worktree_fail(id: u64, error: WorktreeError) -> Response {
    let (code, user_message) = match &error {
        WorktreeError::UnknownRepository(path) => (
            ErrorCode::NotFound,
            format!("Retcon does not have a project registered at {path}."),
        ),
        WorktreeError::NotFound(path) => (
            ErrorCode::NotFound,
            format!("Retcon could not find a worktree record for {path}."),
        ),
        WorktreeError::Git(git) => return git_fail(id, git.clone()),
        WorktreeError::Storage(_) => (
            ErrorCode::Internal,
            "Retcon could not update worktree records.".into(),
        ),
    };
    failed(
        id,
        CoreError::new(
            code,
            ErrorSource::Rpc,
            user_message,
            error.to_string(),
        ),
    )
}

fn repo_param(params: &Value) -> Result<PathBuf, CoreError> {
    params
        .get("repo")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| {
            CoreError::new(
                ErrorCode::InvalidRequest,
                ErrorSource::Rpc,
                "The Git request is missing the repository path.",
                "missing 'repo' parameter",
            )
        })
}

fn str_param<'a>(params: &'a Value, key: &str) -> Result<&'a str, CoreError> {
    params.get(key).and_then(Value::as_str).ok_or_else(|| {
        CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "The Git request is missing required information.",
            format!("missing '{key}' parameter"),
        )
    })
}

fn optional_str_param<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str)
}

fn parse_diff_mode(params: &Value) -> DiffMode {
    match optional_str_param(params, "mode") {
        Some("staged") => DiffMode::Staged,
        Some("all") => DiffMode::All,
        _ => DiffMode::Unstaged,
    }
}

fn parse_session_id(params: &Value) -> Result<Option<Uuid>, CoreError> {
    match optional_str_param(params, "sessionId") {
        Some(raw) => Uuid::parse_str(raw)
            .map(Some)
            .map_err(|error| {
                CoreError::new(
                    ErrorCode::InvalidRequest,
                    ErrorSource::Rpc,
                    "The session ID is invalid.",
                    error.to_string(),
                )
            }),
        None => Ok(None),
    }
}

fn worktrees(state: &CoreState) -> WorktreeManager {
    WorktreeManager::new(state.storage().database().clone())
}

async fn ensure_staged_changes_safe(repo: &Path) -> Result<(), CoreError> {
    let repo = repo.to_path_buf();
    let scan = tokio::task::spawn_blocking(move || retcon_secrets::scan_staged(&repo))
        .await
        .map_err(|error| {
            CoreError::new(
                ErrorCode::Internal,
                ErrorSource::System,
                "Retcon could not complete the staged secret scan.",
                format!("staged secret scan task failed: {error}"),
            )
        })?
        .map_err(|error| {
            CoreError::new(
                ErrorCode::Io,
                ErrorSource::System,
                "Retcon could not inspect the staged Git changes.",
                error.to_string(),
            )
            .suggested_fix("Confirm this is a Git repository and retry the commit.")
        })?;
    if scan.is_clean() {
        return Ok(());
    }
    Err(CoreError::new(
        ErrorCode::PermissionDenied,
        ErrorSource::Rpc,
        "Retcon blocked the commit because staged changes may contain secrets.",
        format!("staged secret scan blocked commit: {}", scan.summary()),
    )
    .suggested_fix("Remove the secret from the staged changes, rotate it if necessary, and retry."))
}

/// Handle a `git.*` request.
pub async fn handle(state: CoreState, request: Request) -> Response {
    let Request { id, method, params } = request;
    let repo = match repo_param(&params) {
        Ok(repo) => repo,
        Err(error) => return failed(id, error),
    };

    match method.as_str() {
        "git.status" => match retcon_git::status(&repo).await {
            Ok(status) => Response::ok(id, json!(status)),
            Err(error) => git_fail(id, error),
        },
        "git.defaultBranch" => match retcon_git::default_branch(&repo).await {
            Ok(branch) => Response::ok(id, json!({ "branch": branch })),
            Err(error) => git_fail(id, error),
        },
        "git.branchList" => match retcon_git::branch_list(&repo).await {
            Ok(branches) => Response::ok(id, json!({ "branches": branches })),
            Err(error) => git_fail(id, error),
        },
        "git.branchCreate" => {
            crate::checkpoints_rpc::hook_git_mutation(&state, &repo, &params).await;
            let branch = match str_param(&params, "branch") {
                Ok(branch) => branch,
                Err(error) => return failed(id, error),
            };
            match retcon_git::branch_create(&repo, branch).await {
                Ok(()) => Response::ok(id, json!({})),
                Err(error) => git_fail(id, error),
            }
        }
        "git.branchDelete" => {
            crate::checkpoints_rpc::hook_git_mutation(&state, &repo, &params).await;
            let branch = match str_param(&params, "branch") {
                Ok(branch) => branch,
                Err(error) => return failed(id, error),
            };
            let force = params
                .get("force")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            match retcon_git::branch_delete(&repo, branch, force).await {
                Ok(()) => Response::ok(id, json!({})),
                Err(error) => git_fail(id, error),
            }
        }
        "git.checkout" => {
            crate::checkpoints_rpc::hook_git_mutation(&state, &repo, &params).await;
            let branch = match str_param(&params, "branch") {
                Ok(branch) => branch,
                Err(error) => return failed(id, error),
            };
            match retcon_git::checkout(&repo, branch).await {
                Ok(()) => Response::ok(id, json!({})),
                Err(error) => git_fail(id, error),
            }
        }
        "git.stage" => {
            crate::checkpoints_rpc::hook_git_mutation(&state, &repo, &params).await;
            let path = optional_str_param(&params, "path");
            match retcon_git::stage(&repo, path).await {
                Ok(()) => Response::ok(id, json!({})),
                Err(error) => git_fail(id, error),
            }
        }
        "git.unstage" => {
            crate::checkpoints_rpc::hook_git_mutation(&state, &repo, &params).await;
            let path = optional_str_param(&params, "path");
            match retcon_git::unstage(&repo, path).await {
                Ok(()) => Response::ok(id, json!({})),
                Err(error) => git_fail(id, error),
            }
        }
        "git.stageHunk" => {
            crate::checkpoints_rpc::hook_git_mutation(&state, &repo, &params).await;
            let patch = match str_param(&params, "patch") {
                Ok(patch) => patch,
                Err(error) => return failed(id, error),
            };
            match retcon_git::stage_hunk(&repo, patch).await {
                Ok(()) => Response::ok(id, json!({})),
                Err(error) => git_fail(id, error),
            }
        }
        "git.discardHunk" => {
            crate::checkpoints_rpc::hook_git_mutation(&state, &repo, &params).await;
            let patch = match str_param(&params, "patch") {
                Ok(patch) => patch,
                Err(error) => return failed(id, error),
            };
            match retcon_git::discard_hunk(&repo, patch).await {
                Ok(()) => Response::ok(id, json!({})),
                Err(error) => git_fail(id, error),
            }
        }
        "git.commit" => {
            let message = match str_param(&params, "message") {
                Ok(message) => message,
                Err(error) => return failed(id, error),
            };
            let allow_empty = params
                .get("allowEmpty")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if let Err(error) = ensure_staged_changes_safe(&repo).await {
                return failed(id, error);
            }
            crate::checkpoints_rpc::hook_git_mutation(&state, &repo, &params).await;
            match retcon_git::commit(&repo, message, allow_empty).await {
                Ok(oid) => Response::ok(id, json!({ "oid": oid })),
                Err(error) => git_fail(id, error),
            }
        }
        "git.push" => {
            crate::checkpoints_rpc::hook_git_mutation(&state, &repo, &params).await;
            let remote = optional_str_param(&params, "remote").unwrap_or("origin");
            let branch = optional_str_param(&params, "branch");
            let force = params
                .get("force")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            match retcon_git::push(&repo, remote, branch, force).await {
                Ok(()) => Response::ok(id, json!({})),
                Err(error) => git_fail(id, error),
            }
        }
        "git.conflicts" => match retcon_git::conflicts(&repo).await {
            Ok(paths) => Response::ok(id, json!({ "paths": paths })),
            Err(error) => git_fail(id, error),
        },
        "git.submodules" => match retcon_git::submodules(&repo).await {
            Ok(paths) => Response::ok(id, json!({ "paths": paths })),
            Err(error) => git_fail(id, error),
        },
        "git.diff" => {
            let path = optional_str_param(&params, "path");
            let mode = parse_diff_mode(&params);
            match retcon_git::diff(&repo, path, mode).await {
                Ok(diff) => Response::ok(id, json!({ "diff": diff, "mode": diff_mode_label(mode) })),
                Err(error) => git_fail(id, error),
            }
        }
        "git.worktreeList" => match retcon_git::worktree_list(&repo).await {
            Ok(worktrees) => Response::ok(id, json!(worktrees)),
            Err(error) => git_fail(id, error),
        },
        "git.worktreeListStored" => match worktrees(&state).list_stored(&repo) {
            Ok(worktrees) => Response::ok(id, json!({ "worktrees": worktrees })),
            Err(error) => worktree_fail(id, error),
        },
        "git.worktreeAdd" => {
            crate::checkpoints_rpc::hook_git_mutation(&state, &repo, &params).await;
            let path = match str_param(&params, "path") {
                Ok(path) => path,
                Err(error) => return failed(id, error),
            };
            let branch = match str_param(&params, "branch") {
                Ok(branch) => branch,
                Err(error) => return failed(id, error),
            };
            let session_id = match parse_session_id(&params) {
                Ok(session_id) => session_id,
                Err(error) => return failed(id, error),
            };
            match worktrees(&state)
                .add(&repo, path, branch, session_id)
                .await
            {
                Ok(record) => Response::ok(id, json!(StoredWorktreeResponse::from(record))),
                Err(error) => worktree_fail(id, error),
            }
        }
        "git.worktreeRemove" => {
            crate::checkpoints_rpc::hook_git_mutation(&state, &repo, &params).await;
            let path = match str_param(&params, "path") {
                Ok(path) => path,
                Err(error) => return failed(id, error),
            };
            match worktrees(&state).remove(&repo, path).await {
                Ok(removed) => Response::ok(id, json!({ "removed": removed })),
                Err(error) => worktree_fail(id, error),
            }
        }
        "git.worktreeAssign" => {
            let path = match str_param(&params, "path") {
                Ok(path) => path,
                Err(error) => return failed(id, error),
            };
            let session_id = match optional_str_param(&params, "sessionId") {
                Some(raw) => match Uuid::parse_str(raw) {
                    Ok(id) => id,
                    Err(error) => {
                        return failed(
                            id,
                            CoreError::new(
                                ErrorCode::InvalidRequest,
                                ErrorSource::Rpc,
                                "The session ID is invalid.",
                                error.to_string(),
                            ),
                        );
                    }
                },
                None => {
                    return failed(
                        id,
                        CoreError::new(
                            ErrorCode::InvalidRequest,
                            ErrorSource::Rpc,
                            "Assigning a worktree needs a session ID.",
                            "missing 'sessionId' parameter",
                        ),
                    );
                }
            };
            match worktrees(&state).assign_session(path, session_id) {
                Ok(record) => Response::ok(id, json!(StoredWorktreeResponse::from(record))),
                Err(error) => worktree_fail(id, error),
            }
        }
        other => failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "The requested Git operation is not available.",
                format!("unknown RPC method: {other}"),
            ),
        ),
    }
}

fn diff_mode_label(mode: DiffMode) -> &'static str {
    match mode {
        DiffMode::Unstaged => "unstaged",
        DiffMode::Staged => "staged",
        DiffMode::All => "all",
    }
}

#[derive(serde::Serialize)]
struct StoredWorktreeResponse {
    id: String,
    session_id: Option<String>,
    path: String,
    branch: Option<String>,
    head_oid: Option<String>,
    status: String,
}

impl From<retcon_storage::GitWorktree> for StoredWorktreeResponse {
    fn from(value: retcon_storage::GitWorktree) -> Self {
        Self {
            id: value.id.to_string(),
            session_id: value.session_id.map(|id| id.to_string()),
            path: value.path,
            branch: value.branch,
            head_oid: value.head_oid,
            status: value.status,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn diff_mode_defaults_to_unstaged() {
        assert_eq!(parse_diff_mode(&json!({})), DiffMode::Unstaged);
        assert_eq!(parse_diff_mode(&json!({"mode":"staged"})), DiffMode::Staged);
    }

    #[tokio::test]
    async fn commit_scan_blocks_staged_secret_without_exposing_value() {
        let directory = tempfile::tempdir().unwrap();
        let init = std::process::Command::new("git")
            .args(["init"])
            .current_dir(directory.path())
            .output()
            .unwrap();
        assert!(init.status.success());
        std::fs::write(
            directory.path().join("config.txt"),
            "API_KEY=not-a-real-secret-for-testing",
        )
        .unwrap();
        let add = std::process::Command::new("git")
            .args(["add", "config.txt"])
            .current_dir(directory.path())
            .output()
            .unwrap();
        assert!(add.status.success());

        let error = ensure_staged_changes_safe(directory.path())
            .await
            .unwrap_err();

        assert_eq!(error.code, ErrorCode::PermissionDenied);
        assert!(error.user_message.contains("blocked the commit"));
        assert!(!error.technical_message.contains("not-a-real-secret-for-testing"));
    }
}
