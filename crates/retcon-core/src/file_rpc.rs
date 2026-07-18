//! RPC adapter for filesystem operations.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::{Request, Response};
use crate::state::CoreState;
use retcon_filesystem::{
    DEFAULT_READ_LIMIT, DEFAULT_WRITE_LIMIT, FileWriteCondition, FilesystemError, new_watch_id,
};

fn failed(id: u64, error: CoreError) -> Response {
    Response::error(id, &error)
}

fn map_error(error: FilesystemError) -> CoreError {
    match error {
        FilesystemError::NotFound(path) => CoreError::new(
            ErrorCode::NotFound,
            ErrorSource::Rpc,
            "Retcon could not find that file or folder.",
            format!("path not found: {}", path.display()),
        ),
        FilesystemError::OutsideRoot(path) => CoreError::new(
            ErrorCode::PermissionDenied,
            ErrorSource::Rpc,
            "That path is outside the open project.",
            format!("path outside root: {}", path.display()),
        ),
        FilesystemError::TooLarge { path, size, limit } => CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "That file is too large to open in the editor.",
            format!(
                "file too large: {} ({} bytes, limit {})",
                path.display(),
                size,
                limit
            ),
        )
        .suggested_fix("Use an external editor for very large files."),
        FilesystemError::InvalidRequest(message) => CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "The file request is invalid.",
            message,
        ),
        FilesystemError::Conflict { current_revision } => CoreError::new(
            ErrorCode::Conflict,
            ErrorSource::Rpc,
            "That file changed on disk. Reload it before saving again.",
            "conditional file write revision mismatch",
        )
        .retryable(true)
        .diagnostic(json!({"currentRevision": current_revision})),
        FilesystemError::Watch(message) => CoreError::new(
            ErrorCode::Internal,
            ErrorSource::Rpc,
            "Retcon could not watch that folder for changes.",
            message,
        ),
        FilesystemError::Io(error) => CoreError::io("filesystem operation", error),
    }
}

fn root_param(state: &CoreState, params: &Value) -> Result<PathBuf, CoreError> {
    let requested = params
        .get("root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| {
            CoreError::new(
                ErrorCode::InvalidRequest,
                ErrorSource::Rpc,
                "The file request is missing a project root.",
                "missing 'root' parameter",
            )
        })?;
    let root = std::fs::canonicalize(&requested).map_err(|error| {
        CoreError::new(
            ErrorCode::NotFound,
            ErrorSource::Rpc,
            "The open project folder is unavailable.",
            format!("canonicalize file RPC root: {error}"),
        )
    })?;
    if !root.is_dir() {
        return Err(CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "The file request root must be an open project or worktree folder.",
            "file RPC root is not a directory",
        ));
    }
    let root_text = root.to_string_lossy();
    if state
        .storage()
        .database()
        .projects()
        .find_by_workspace_path(&root_text)?
        .is_none()
    {
        return Err(CoreError::new(
            ErrorCode::PermissionDenied,
            ErrorSource::Rpc,
            "Open this project in Retcon before accessing its files.",
            "file RPC root is not owned by an active project or tracked worktree",
        ));
    }
    Ok(root)
}

fn path_param(params: &Value) -> Result<&str, CoreError> {
    params.get("path").and_then(Value::as_str).ok_or_else(|| {
        CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "The file request is missing a path.",
            "missing 'path' parameter",
        )
    })
}

fn write_condition(params: &Value) -> Result<FileWriteCondition, CoreError> {
    let if_match = params.get("ifMatch").and_then(Value::as_str);
    let if_none_match = params
        .get("ifNoneMatch")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match (if_match, if_none_match) {
        (Some(revision), false) if !revision.is_empty() => {
            Ok(FileWriteCondition::IfMatch(revision.to_owned()))
        }
        (None, true) => Ok(FileWriteCondition::IfNoneMatch),
        _ => Err(CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "Choose whether to update the version you opened or create a new file.",
            "file.write requires exactly one of non-empty 'ifMatch' or 'ifNoneMatch: true'",
        )),
    }
}

pub async fn handle(state: CoreState, request: Request) -> Response {
    let Request { id, method, params } = request;
    let service = state.filesystem().service();
    match method.as_str() {
        "file.list" => {
            let root = match root_param(&state, &params) {
                Ok(root) => root,
                Err(error) => return failed(id, error),
            };
            let path = params.get("path").and_then(Value::as_str);
            match service.list(&root, path).await {
                Ok(entries) => Response::ok(id, json!({"entries": entries})),
                Err(error) => failed(id, map_error(error)),
            }
        }
        "file.read" => {
            let root = match root_param(&state, &params) {
                Ok(root) => root,
                Err(error) => return failed(id, error),
            };
            let path = match path_param(&params) {
                Ok(path) => path,
                Err(error) => return failed(id, error),
            };
            let limit = params
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_READ_LIMIT);
            match service.read(&root, path, limit) {
                Ok(result) => Response::ok(
                    id,
                    serde_json::to_value(result).unwrap_or_else(|_| json!({})),
                ),
                Err(error) => failed(id, map_error(error)),
            }
        }
        "file.write" => {
            let root = match root_param(&state, &params) {
                Ok(root) => root,
                Err(error) => return failed(id, error),
            };
            let path = match path_param(&params) {
                Ok(path) => path,
                Err(error) => return failed(id, error),
            };
            let content = match params.get("content").and_then(Value::as_str) {
                Some(content) => content,
                None => {
                    return failed(
                        id,
                        CoreError::new(
                            ErrorCode::InvalidRequest,
                            ErrorSource::Rpc,
                            "The file write request is missing content.",
                            "missing 'content' parameter",
                        ),
                    );
                }
            };
            let limit = params
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_WRITE_LIMIT);
            let condition = match write_condition(&params) {
                Ok(condition) => condition,
                Err(error) => return failed(id, error),
            };
            let snapshot =
                crate::checkpoints_rpc::hook_file_write_begin(&state, &root, path, &params);
            match service.write(&root, path, content, limit, condition) {
                Ok(result) => {
                    if let Some(snapshot) = snapshot {
                        crate::checkpoints_rpc::hook_file_write_finish(&state, &root, snapshot);
                    }
                    Response::ok(
                        id,
                        serde_json::to_value(result).unwrap_or_else(|_| json!({})),
                    )
                }
                Err(error) => failed(id, map_error(error)),
            }
        }
        "file.watch" => {
            let root = match root_param(&state, &params) {
                Ok(root) => root,
                Err(error) => return failed(id, error),
            };
            let watch_id = params
                .get("watchId")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(new_watch_id);
            let path = params.get("path").and_then(Value::as_str);
            let state_for_emit = state.clone();
            let emit = Arc::new(move |event| {
                state_for_emit.emit(
                    "file.changed",
                    serde_json::to_value(&event).unwrap_or_else(|_| json!({})),
                );
            });
            match state
                .filesystem()
                .watches()
                .watch(watch_id.clone(), &root, path, emit)
            {
                Ok(handle) => Response::ok(id, handle.to_json()),
                Err(error) => failed(id, map_error(error)),
            }
        }
        "file.unwatch" => {
            let watch_id = match params.get("watchId").and_then(Value::as_str) {
                Some(watch_id) => watch_id,
                None => {
                    return failed(
                        id,
                        CoreError::new(
                            ErrorCode::InvalidRequest,
                            ErrorSource::Rpc,
                            "The file watch request is missing an identifier.",
                            "missing 'watchId' parameter",
                        ),
                    );
                }
            };
            let stopped = state.filesystem().watches().unwatch(watch_id);
            Response::ok(id, json!({"stopped": stopped}))
        }
        _ => failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "The requested file operation is not available.",
                format!("unknown file RPC method: {method}"),
            ),
        ),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn file_rpc_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(root.join("hello.txt"), "hello").unwrap();
        let state = CoreState::new(root).unwrap();
        crate::projects::open(&state, &root.to_string_lossy())
            .await
            .unwrap();

        let list = handle(
            state.clone(),
            Request {
                id: 1,
                method: "file.list".into(),
                params: json!({"root": root.to_string_lossy()}),
            },
        )
        .await;
        assert!(list.result.unwrap().to_string().contains("hello.txt"));

        let read = handle(
            state.clone(),
            Request {
                id: 2,
                method: "file.read".into(),
                params: json!({
                    "root": root.to_string_lossy(),
                    "path": "hello.txt",
                }),
            },
        )
        .await;
        let read_result = read.result.unwrap();
        assert!(read_result.to_string().contains("hello"));
        let revision = read_result["revision"].as_str().unwrap();

        let write = handle(
            state.clone(),
            Request {
                id: 3,
                method: "file.write".into(),
                params: json!({
                    "root": root.to_string_lossy(),
                    "path": "hello.txt",
                    "content": "updated",
                    "ifMatch": revision,
                }),
            },
        )
        .await;
        assert!(write.result.is_some());
    }

    #[tokio::test]
    async fn file_write_requires_a_concurrency_condition() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("hello.txt"), "hello").unwrap();
        let state = CoreState::new(temp.path()).unwrap();
        crate::projects::open(&state, &temp.path().to_string_lossy())
            .await
            .unwrap();
        let response = handle(
            state,
            Request {
                id: 1,
                method: "file.write".into(),
                params: json!({
                    "root": temp.path().to_string_lossy(),
                    "path": "hello.txt",
                    "content": "blind overwrite",
                }),
            },
        )
        .await;
        let error = response.error.unwrap();
        assert_eq!(error["code"], "invalid_request");
        assert_eq!(
            fs::read_to_string(temp.path().join("hello.txt")).unwrap(),
            "hello"
        );
    }

    #[tokio::test]
    async fn stale_file_write_returns_only_the_current_revision() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("hello.txt");
        fs::write(&path, "first").unwrap();
        let state = CoreState::new(temp.path()).unwrap();
        crate::projects::open(&state, &temp.path().to_string_lossy())
            .await
            .unwrap();
        let stale = state
            .filesystem()
            .service()
            .read(temp.path(), "hello.txt", DEFAULT_READ_LIMIT)
            .unwrap()
            .revision;
        fs::write(&path, "private current content").unwrap();

        let response = handle(
            state,
            Request {
                id: 1,
                method: "file.write".into(),
                params: json!({
                    "root": temp.path().to_string_lossy(),
                    "path": "hello.txt",
                    "content": "stale",
                    "ifMatch": stale,
                }),
            },
        )
        .await;
        let error = response.error.unwrap();
        assert_eq!(error["code"], "conflict");
        assert_eq!(
            error["diagnostic"]["currentRevision"]
                .as_str()
                .map(str::len),
            Some(64)
        );
        assert!(!error.to_string().contains("private current content"));
        assert_eq!(fs::read_to_string(path).unwrap(), "private current content");
    }

    #[tokio::test]
    async fn file_write_creates_only_with_if_none_match() {
        let temp = tempfile::tempdir().unwrap();
        let state = CoreState::new(temp.path()).unwrap();
        crate::projects::open(&state, &temp.path().to_string_lossy())
            .await
            .unwrap();
        let response = handle(
            state,
            Request {
                id: 1,
                method: "file.write".into(),
                params: json!({
                    "root": temp.path().to_string_lossy(),
                    "path": "created.txt",
                    "content": "new",
                    "ifNoneMatch": true,
                }),
            },
        )
        .await;
        assert!(response.result.is_some());
        assert_eq!(
            fs::read_to_string(temp.path().join("created.txt")).unwrap(),
            "new"
        );
    }

    #[tokio::test]
    async fn file_read_rejects_a_caller_selected_unopened_root() {
        let data = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("private.txt"), "private").unwrap();
        let state = CoreState::new(data.path()).unwrap();

        let response = handle(
            state,
            Request {
                id: 1,
                method: "file.read".into(),
                params: json!({
                    "root": outside.path().to_string_lossy(),
                    "path": "private.txt",
                }),
            },
        )
        .await;

        assert_eq!(response.error.unwrap()["code"], "permission_denied");
    }

    #[tokio::test]
    async fn file_read_accepts_a_legacy_relative_tracked_worktree() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repository");
        let worktree = temp.path().join("feature");
        fs::create_dir(&repository).unwrap();
        fs::create_dir(&worktree).unwrap();
        fs::write(worktree.join("tracked.txt"), "tracked").unwrap();
        let state = CoreState::new(&temp.path().join("data")).unwrap();
        crate::projects::open(&state, &repository.to_string_lossy())
            .await
            .unwrap();
        let location_id = state
            .storage()
            .database()
            .projects()
            .location_id_by_path(&repository.canonicalize().unwrap().to_string_lossy())
            .unwrap()
            .unwrap();
        state
            .storage()
            .database()
            .git_worktrees()
            .create(&retcon_storage::NewGitWorktree::new(
                location_id,
                "../feature",
            ))
            .unwrap();

        let response = handle(
            state,
            Request {
                id: 1,
                method: "file.read".into(),
                params: json!({
                    "root": worktree.to_string_lossy(),
                    "path": "tracked.txt",
                }),
            },
        )
        .await;

        assert_eq!(response.result.unwrap()["content"], "tracked");
    }
}
