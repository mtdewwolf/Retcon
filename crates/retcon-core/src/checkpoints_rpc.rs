//! RPC adapter for checkpoint capture and rollback.

use std::path::PathBuf;

use retcon_checkpoints::{CheckpointError, CheckpointKind, CheckpointService};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::{Request, Response};
use crate::state::CoreState;

fn failed(id: u64, error: CoreError) -> Response {
    Response::error(id, &error)
}

fn map_error(error: CheckpointError) -> CoreError {
    match error {
        CheckpointError::UnknownRepository(path) => CoreError::new(
            ErrorCode::NotFound,
            ErrorSource::Rpc,
            "Retcon does not have a project registered at that path.",
            format!("unknown repository: {path}"),
        ),
        CheckpointError::NotFound(id) => CoreError::new(
            ErrorCode::NotFound,
            ErrorSource::Rpc,
            "Retcon could not find that checkpoint.",
            format!("checkpoint not found: {id}"),
        ),
        CheckpointError::OutsideRoot(path) => CoreError::new(
            ErrorCode::PermissionDenied,
            ErrorSource::Rpc,
            "That path is outside the open project.",
            format!("path outside root: {path}"),
        ),
        CheckpointError::Conflicts(paths) => CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "Rollback would overwrite unrelated edits.",
            format!("rollback conflicts: {paths}"),
        )
        .suggested_fix("Review the rollback preview and restore only the paths you intend to revert, or pass force=true after confirming."),
        CheckpointError::InvalidRequest(message) => CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "The checkpoint request is invalid.",
            message,
        ),
        CheckpointError::Io(error) => CoreError::io("checkpoint filesystem operation", error),
        CheckpointError::Storage(error) => CoreError::new(
            ErrorCode::Internal,
            ErrorSource::Rpc,
            "Retcon could not access checkpoint storage.",
            error.to_string(),
        ),
        CheckpointError::Git(error) => CoreError::new(
            ErrorCode::Io,
            ErrorSource::Rpc,
            "Retcon could not capture Git state for the checkpoint.",
            error.to_string(),
        ),
    }
}

fn checkpoints(state: &CoreState) -> CheckpointService {
    CheckpointService::new(
        state.storage().database().clone(),
        state.storage().artifacts().clone(),
    )
}

fn root_param(params: &Value) -> Result<PathBuf, CoreError> {
    params
        .get("root")
        .or_else(|| params.get("repo"))
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| {
            CoreError::new(
                ErrorCode::InvalidRequest,
                ErrorSource::Rpc,
                "The checkpoint request is missing a project root.",
                "missing 'root' or 'repo' parameter",
            )
        })
}

fn parse_uuid_param(params: &Value, key: &str) -> Result<Option<Uuid>, CoreError> {
    match params.get(key).and_then(Value::as_str) {
        Some(raw) => Uuid::parse_str(raw).map(Some).map_err(|error| {
            CoreError::new(
                ErrorCode::InvalidRequest,
                ErrorSource::Rpc,
                "A checkpoint identifier is invalid.",
                format!("invalid {key}: {error}"),
            )
        }),
        None => Ok(None),
    }
}

fn checkpoint_to_json(checkpoint: &retcon_storage::GitCheckpoint) -> Value {
    json!({
        "id": checkpoint.id.to_string(),
        "gitWorktreeId": checkpoint.git_worktree_id.to_string(),
        "turnId": checkpoint.turn_id.map(|id| id.to_string()),
        "kind": checkpoint.kind,
        "baseOid": checkpoint.base_oid,
        "patchArtifactHash": checkpoint.patch_artifact_hash,
        "createdAt": checkpoint.created_at,
    })
}

fn file_change_to_json(change: &retcon_storage::FileChange) -> Value {
    json!({
        "id": change.id.to_string(),
        "turnId": change.turn_id.map(|id| id.to_string()),
        "gitCheckpointId": change.git_checkpoint_id.map(|id| id.to_string()),
        "path": change.path,
        "changeKind": change.change_kind,
        "beforeArtifactHash": change.before_artifact_hash,
        "afterArtifactHash": change.after_artifact_hash,
        "createdAt": change.created_at,
    })
}

pub async fn handle(state: CoreState, request: Request) -> Response {
    let Request { id, method, params } = request;
    let service = checkpoints(&state);
    match method.as_str() {
        "checkpoint.create" => {
            let root = match root_param(&params) {
                Ok(root) => root,
                Err(error) => return failed(id, error),
            };
            let turn_id = match parse_uuid_param(&params, "turnId") {
                Ok(turn_id) => turn_id,
                Err(error) => return failed(id, error),
            };
            let kind = params
                .get("kind")
                .and_then(Value::as_str)
                .and_then(CheckpointKind::parse)
                .unwrap_or(CheckpointKind::Manual);
            let result = match kind {
                CheckpointKind::TurnStart => {
                    let Some(turn_id) = turn_id else {
                        return failed(
                            id,
                            CoreError::new(
                                ErrorCode::InvalidRequest,
                                ErrorSource::Rpc,
                                "Turn-start checkpoints require a turn ID.",
                                "missing 'turnId' parameter",
                            ),
                        );
                    };
                    service.create_turn_start(&root, turn_id).await
                }
                CheckpointKind::Manual => service.create_manual(&root, turn_id).await,
                CheckpointKind::PreGit => service.before_git_operation(&root, turn_id).await,
                other => {
                    return failed(
                        id,
                        CoreError::new(
                            ErrorCode::InvalidRequest,
                            ErrorSource::Rpc,
                            "That checkpoint kind cannot be created directly.",
                            format!("unsupported checkpoint kind: {other}"),
                        ),
                    );
                }
            };
            match result {
                Ok(checkpoint) => Response::ok(id, checkpoint_to_json(&checkpoint)),
                Err(error) => failed(id, map_error(error)),
            }
        }
        "checkpoint.list" => {
            let root = match root_param(&params) {
                Ok(root) => root,
                Err(error) => return failed(id, error),
            };
            let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
            if let Some(turn_id) = match parse_uuid_param(&params, "turnId") {
                Ok(turn_id) => turn_id,
                Err(error) => return failed(id, error),
            } {
                match service.list_for_turn(turn_id) {
                    Ok(checkpoints) => Response::ok(
                        id,
                        json!({"checkpoints": checkpoints.iter().map(checkpoint_to_json).collect::<Vec<_>>()}),
                    ),
                    Err(error) => failed(id, map_error(error)),
                }
            } else {
                match service.list_for_root(&root, limit) {
                    Ok(checkpoints) => Response::ok(
                        id,
                        json!({"checkpoints": checkpoints.iter().map(checkpoint_to_json).collect::<Vec<_>>()}),
                    ),
                    Err(error) => failed(id, map_error(error)),
                }
            }
        }
        "checkpoint.get" => {
            let checkpoint_id = match parse_uuid_param(&params, "checkpointId") {
                Ok(Some(checkpoint_id)) => checkpoint_id,
                Ok(None) => {
                    return failed(
                        id,
                        CoreError::new(
                            ErrorCode::InvalidRequest,
                            ErrorSource::Rpc,
                            "The checkpoint request is missing an identifier.",
                            "missing 'checkpointId' parameter",
                        ),
                    );
                }
                Err(error) => return failed(id, error),
            };
            match service.get(checkpoint_id) {
                Ok(Some(checkpoint)) => {
                    let changes = service
                        .file_changes_for_checkpoint(checkpoint_id)
                        .unwrap_or_default();
                    Response::ok(
                        id,
                        json!({
                            "checkpoint": checkpoint_to_json(&checkpoint),
                            "fileChanges": changes.iter().map(file_change_to_json).collect::<Vec<_>>(),
                        }),
                    )
                }
                Ok(None) => failed(
                    id,
                    map_error(CheckpointError::NotFound(checkpoint_id.to_string())),
                ),
                Err(error) => failed(id, map_error(error)),
            }
        }
        "checkpoint.preview" => {
            let root = match root_param(&params) {
                Ok(root) => root,
                Err(error) => return failed(id, error),
            };
            let checkpoint_id = match parse_uuid_param(&params, "checkpointId") {
                Ok(Some(checkpoint_id)) => checkpoint_id,
                Ok(None) => {
                    return failed(
                        id,
                        CoreError::new(
                            ErrorCode::InvalidRequest,
                            ErrorSource::Rpc,
                            "Rollback preview needs a checkpoint identifier.",
                            "missing 'checkpointId' parameter",
                        ),
                    );
                }
                Err(error) => return failed(id, error),
            };
            match service.preview_rollback(&root, checkpoint_id) {
                Ok(preview) => Response::ok(
                    id,
                    serde_json::to_value(preview).unwrap_or_else(|_| json!({})),
                ),
                Err(error) => failed(id, map_error(error)),
            }
        }
        "checkpoint.restore" => {
            let root = match root_param(&params) {
                Ok(root) => root,
                Err(error) => return failed(id, error),
            };
            let checkpoint_id = match parse_uuid_param(&params, "checkpointId") {
                Ok(Some(checkpoint_id)) => checkpoint_id,
                Ok(None) => {
                    return failed(
                        id,
                        CoreError::new(
                            ErrorCode::InvalidRequest,
                            ErrorSource::Rpc,
                            "Restore needs a checkpoint identifier.",
                            "missing 'checkpointId' parameter",
                        ),
                    );
                }
                Err(error) => return failed(id, error),
            };
            let paths = params
                .get("paths")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if paths.is_empty() {
                return failed(
                    id,
                    CoreError::new(
                        ErrorCode::InvalidRequest,
                        ErrorSource::Rpc,
                        "Restore needs at least one file path.",
                        "missing 'paths' parameter",
                    ),
                );
            }
            let force = params
                .get("force")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            match service.restore_selective(&root, checkpoint_id, &paths, force) {
                Ok(report) => Response::ok(
                    id,
                    serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                ),
                Err(error) => failed(id, map_error(error)),
            }
        }
        _ => failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "The requested checkpoint operation is not available.",
                format!("unknown checkpoint RPC method: {method}"),
            ),
        ),
    }
}

/// Best-effort checkpoint hook for mutating Git RPC methods.
pub async fn hook_git_mutation(state: &CoreState, repo: &std::path::Path, params: &Value) {
    let turn_id = params
        .get("turnId")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok());
    let service = checkpoints(state);
    if let Ok(checkpoint) = service.before_git_operation(repo, turn_id).await {
        state.emit(
            "checkpoint.created",
            json!({
                "checkpointId": checkpoint.id.to_string(),
                "kind": checkpoint.kind,
                "turnId": checkpoint.turn_id.map(|id| id.to_string()),
            }),
        );
    }
}

/// Best-effort checkpoint hook for file writes.
pub fn hook_file_write_begin(
    state: &CoreState,
    root: &std::path::Path,
    path: &str,
    params: &Value,
) -> Option<retcon_checkpoints::FileWriteCheckpoint> {
    let turn_id = params
        .get("turnId")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok());
    let service = checkpoints(state);
    match service.begin_file_write(root, path, turn_id) {
        Ok(snapshot) => Some(snapshot),
        Err(error) => {
            tracing::debug!(%error, "file write checkpoint skipped");
            None
        }
    }
}

/// Finalize a file-write checkpoint after the write succeeds.
pub fn hook_file_write_finish(
    state: &CoreState,
    root: &std::path::Path,
    snapshot: retcon_checkpoints::FileWriteCheckpoint,
) {
    let service = checkpoints(state);
    match service.finish_file_write(root, snapshot.clone()) {
        Ok(change) => {
            state.emit(
                "checkpoint.created",
                json!({
                    "checkpointId": snapshot.checkpoint_id.to_string(),
                    "kind": "pre-file-write",
                    "path": change.path,
                    "turnId": change.turn_id.map(|id| id.to_string()),
                }),
            );
        }
        Err(error) => tracing::debug!(%error, "file write checkpoint finalize skipped"),
    }
}
