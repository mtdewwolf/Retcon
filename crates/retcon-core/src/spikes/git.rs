//! Git spike handlers: `git.*` methods over the Git CLI crate.

use std::path::PathBuf;

use serde_json::{Value, json};

use crate::rpc::Response;

fn repo(params: &Value) -> Option<PathBuf> {
    params.get("repo").and_then(Value::as_str).map(PathBuf::from)
}

fn git_err(id: u64, e: retcon_git::GitError) -> Response {
    let mut r = Response::err(id, "git_failed", e.to_string());
    if let Some(error) = r.error.as_mut() {
        error.message = format!("git {}: {}", e.command, e.stderr);
    }
    r
}

/// Handle a `git.*` request.
pub async fn handle(id: u64, method: &str, params: Value) -> Response {
    let Some(repo) = repo(&params) else {
        return Response::err(id, "bad_params", "missing 'repo' path");
    };
    match method {
        "git.status" => match retcon_git::status(&repo).await {
            Ok(s) => Response::ok(id, json!(s)),
            Err(e) => git_err(id, e),
        },
        "git.worktreeList" => match retcon_git::worktree_list(&repo).await {
            Ok(w) => Response::ok(id, json!(w)),
            Err(e) => git_err(id, e),
        },
        "git.worktreeAdd" => {
            let path = params.get("path").and_then(Value::as_str).unwrap_or_default();
            let branch = params.get("branch").and_then(Value::as_str).unwrap_or_default();
            if path.is_empty() || branch.is_empty() {
                return Response::err(id, "bad_params", "need 'path' and 'branch'");
            }
            match retcon_git::worktree_add(&repo, path, branch).await {
                Ok(()) => Response::ok(id, json!({})),
                Err(e) => git_err(id, e),
            }
        }
        "git.worktreeRemove" => {
            let path = params.get("path").and_then(Value::as_str).unwrap_or_default();
            if path.is_empty() {
                return Response::err(id, "bad_params", "need 'path'");
            }
            match retcon_git::worktree_remove(&repo, path).await {
                Ok(()) => Response::ok(id, json!({})),
                Err(e) => git_err(id, e),
            }
        }
        "git.diff" => {
            let path = params.get("path").and_then(Value::as_str);
            match retcon_git::diff(&repo, path).await {
                Ok(d) => Response::ok(id, json!({ "diff": d })),
                Err(e) => git_err(id, e),
            }
        }
        _ => Response::err(id, "not_found", format!("unknown method {method}")),
    }
}
