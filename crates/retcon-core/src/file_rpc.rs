//! RPC adapter for filesystem operations.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::{Request, Response};
use crate::state::CoreState;
use retcon_filesystem::{
    DEFAULT_READ_LIMIT, DEFAULT_WRITE_LIMIT, FilesystemError, new_watch_id,
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
        FilesystemError::Watch(message) => CoreError::new(
            ErrorCode::Internal,
            ErrorSource::Rpc,
            "Retcon could not watch that folder for changes.",
            message,
        ),
        FilesystemError::Io(error) => CoreError::io("filesystem operation", error),
    }
}

fn root_param(params: &Value) -> Result<PathBuf, CoreError> {
    params
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
        })
}

fn path_param(params: &Value) -> Result<&str, CoreError> {
    params
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CoreError::new(
                ErrorCode::InvalidRequest,
                ErrorSource::Rpc,
                "The file request is missing a path.",
                "missing 'path' parameter",
            )
        })
}

pub async fn handle(state: CoreState, request: Request) -> Response {
    let Request { id, method, params } = request;
    let service = state.filesystem().service();
    match method.as_str() {
        "file.list" => {
            let root = match root_param(&params) {
                Ok(root) => root,
                Err(error) => return failed(id, error),
            };
            let path = params.get("path").and_then(Value::as_str);
            match service.list(&root, path) {
                Ok(entries) => Response::ok(id, json!({"entries": entries})),
                Err(error) => failed(id, map_error(error)),
            }
        }
        "file.read" => {
            let root = match root_param(&params) {
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
            let root = match root_param(&params) {
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
            let snapshot =
                crate::checkpoints_rpc::hook_file_write_begin(&state, &root, path, &params);
            match service.write(&root, path, content, limit) {
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
            let root = match root_param(&params) {
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
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn file_rpc_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(root.join("hello.txt"), "hello").unwrap();
        let state = CoreState::new(root).unwrap();

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
        assert!(read.result.unwrap().to_string().contains("hello"));

        let write = handle(
            state.clone(),
            Request {
                id: 3,
                method: "file.write".into(),
                params: json!({
                    "root": root.to_string_lossy(),
                    "path": "hello.txt",
                    "content": "updated",
                }),
            },
        )
        .await;
        assert!(write.result.is_some());
    }
}
