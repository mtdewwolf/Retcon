//! Git spike handlers: `git.*` methods over the Git CLI crate.

use std::path::PathBuf;

use retcon_git::DiffMode;

use serde_json::json;

use super::{fail, param_str};
use crate::error::ErrorCode;
use crate::rpc::{Request, Response};

fn git_fail(id: u64, e: retcon_git::GitError) -> Response {
    fail(
        id,
        ErrorCode::Io,
        "The Git operation failed.",
        format!("git {} (exit {:?}): {}", e.command, e.exit_code, e.stderr),
    )
}

/// Handle a `git.*` request.
pub async fn handle(request: Request) -> Response {
    let Request { id, method, params } = request;
    let Some(repo) = param_str(&params, "repo").map(PathBuf::from) else {
        return fail(
            id,
            ErrorCode::InvalidRequest,
            "The Git request is missing the repository path.",
            "missing 'repo' parameter",
        );
    };
    match method.as_str() {
        "git.status" => match retcon_git::status(&repo).await {
            Ok(s) => Response::ok(id, json!(s)),
            Err(e) => git_fail(id, e),
        },
        "git.branchCreate" => {
            let branch = param_str(&params, "branch").unwrap_or_default();
            if branch.is_empty() {
                return fail(
                    id,
                    ErrorCode::InvalidRequest,
                    "Creating a branch needs a name.",
                    "missing 'branch' parameter",
                );
            }
            match retcon_git::branch_create(&repo, branch).await {
                Ok(()) => Response::ok(id, json!({})),
                Err(e) => git_fail(id, e),
            }
        }
        "git.conflicts" => match retcon_git::conflicts(&repo).await {
            Ok(paths) => Response::ok(id, json!({"paths": paths})),
            Err(e) => git_fail(id, e),
        },
        "git.submodules" => match retcon_git::submodules(&repo).await {
            Ok(paths) => Response::ok(id, json!({"paths": paths})),
            Err(e) => git_fail(id, e),
        },
        "git.worktreeList" => match retcon_git::worktree_list(&repo).await {
            Ok(w) => Response::ok(id, json!(w)),
            Err(e) => git_fail(id, e),
        },
        "git.worktreeAdd" => {
            let path = param_str(&params, "path").unwrap_or_default();
            let branch = param_str(&params, "branch").unwrap_or_default();
            if path.is_empty() || branch.is_empty() {
                return fail(
                    id,
                    ErrorCode::InvalidRequest,
                    "Creating a worktree needs both a path and a branch name.",
                    "missing 'path' or 'branch' parameter",
                );
            }
            match retcon_git::worktree_add(&repo, path, branch).await {
                Ok(()) => Response::ok(id, json!({})),
                Err(e) => git_fail(id, e),
            }
        }
        "git.worktreeRemove" => {
            let path = param_str(&params, "path").unwrap_or_default();
            if path.is_empty() {
                return fail(
                    id,
                    ErrorCode::InvalidRequest,
                    "Removing a worktree needs its path.",
                    "missing 'path' parameter",
                );
            }
            match retcon_git::worktree_remove(&repo, path).await {
                Ok(()) => Response::ok(id, json!({})),
                Err(e) => git_fail(id, e),
            }
        }
        "git.diff" => {
            match retcon_git::diff(&repo, param_str(&params, "path"), DiffMode::Unstaged).await {
                Ok(d) => Response::ok(id, json!({ "diff": d })),
                Err(e) => git_fail(id, e),
            }
        }
        other => fail(
            id,
            ErrorCode::NotFound,
            "The requested Git operation is not available.",
            format!("unknown RPC method: {other}"),
        ),
    }
}
