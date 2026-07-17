//! RPC adapter for storage health, recovery, and layout persistence.

use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::{Request, Response};
use crate::state::CoreState;
use retcon_storage::{NewLayout, RecoverAction, RecoverOptions};

fn failed(id: u64, error: CoreError) -> Response {
    Response::error(id, &error)
}

fn parse_action(value: Option<&Value>) -> Result<RecoverAction, CoreError> {
    match value.and_then(Value::as_str).unwrap_or("report") {
        "report" => Ok(RecoverAction::Report),
        "backup" => Ok(RecoverAction::Backup),
        "repair" => Ok(RecoverAction::Repair),
        "reset" => Ok(RecoverAction::Reset),
        other => Err(CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "The recovery action is not supported.",
            format!("unknown recovery action: {other}"),
        )),
    }
}

fn layout_to_json(layout: &retcon_storage::Layout) -> Value {
    json!({
        "id": layout.id,
        "workspaceId": layout.workspace_id,
        "name": layout.name,
        "layout": layout.layout,
        "isActive": layout.is_active,
        "updatedAt": layout.updated_at,
    })
}

pub async fn handle(state: CoreState, request: Request) -> Response {
    let Request { id, method, params } = request;
    match method.as_str() {
        "storage.status" => match state.storage().status() {
            Ok(report) => Response::ok(
                id,
                serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            ),
            Err(error) => failed(id, error.into()),
        },
        "storage.recover" => {
            let action = match parse_action(params.get("action")) {
                Ok(action) => action,
                Err(error) => return failed(id, error),
            };
            let backup_path = params
                .get("backupPath")
                .and_then(Value::as_str)
                .map(std::path::PathBuf::from);
            let options = RecoverOptions {
                action,
                backup_path,
            };
            match state.storage().recover(options) {
                Ok(report) => Response::ok(
                    id,
                    serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                ),
                Err(error) => failed(id, error.into()),
            }
        }
        "storage.layout.get" => {
            let workspace_id = match params.get("workspaceId") {
                Some(value) if value.is_null() => None,
                Some(value) => match value.as_str() {
                    Some(raw) => match Uuid::parse_str(raw) {
                        Ok(id) => Some(id),
                        Err(error) => {
                            return failed(
                                id,
                                CoreError::new(
                                    ErrorCode::InvalidRequest,
                                    ErrorSource::Rpc,
                                    "The workspace ID is invalid.",
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
                                "The workspace ID must be a string.",
                                "workspaceId must be a UUID string or null",
                            ),
                        );
                    }
                },
                None => None,
            };
            let layout_id = params.get("layoutId").and_then(Value::as_str);
            match layout_id {
                Some(layout_id) => match state.storage().database().layouts().get(layout_id) {
                    Ok(layout) => {
                        Response::ok(id, json!({"layout": layout.map(|l| layout_to_json(&l))}))
                    }
                    Err(error) => failed(id, error.into()),
                },
                None => {
                    match state
                        .storage()
                        .database()
                        .layouts()
                        .get_active(workspace_id)
                    {
                        Ok(layout) => {
                            Response::ok(id, json!({"layout": layout.map(|l| layout_to_json(&l))}))
                        }
                        Err(error) => failed(id, error.into()),
                    }
                }
            }
        }
        "storage.layout.save" => {
            let layout_id = match params.get("layoutId").and_then(Value::as_str) {
                Some(id) => id.to_owned(),
                None => {
                    return failed(
                        id,
                        CoreError::new(
                            ErrorCode::InvalidRequest,
                            ErrorSource::Rpc,
                            "The layout request is missing a layout ID.",
                            "missing 'layoutId' parameter",
                        ),
                    );
                }
            };
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Default");
            let layout = match params.get("layout") {
                Some(value) => value.clone(),
                None => {
                    return failed(
                        id,
                        CoreError::new(
                            ErrorCode::InvalidRequest,
                            ErrorSource::Rpc,
                            "The layout request is missing layout data.",
                            "missing 'layout' parameter",
                        ),
                    );
                }
            };
            let workspace_id = params
                .get("workspaceId")
                .and_then(Value::as_str)
                .map(Uuid::parse_str)
                .transpose()
                .map_err(|error| {
                    CoreError::new(
                        ErrorCode::InvalidRequest,
                        ErrorSource::Rpc,
                        "The workspace ID is invalid.",
                        error.to_string(),
                    )
                });
            let workspace_id = match workspace_id {
                Ok(value) => value,
                Err(error) => return failed(id, error),
            };
            let is_active = params
                .get("isActive")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let saved = NewLayout {
                id: layout_id,
                workspace_id,
                name: name.into(),
                layout,
                is_active,
            };
            match state.storage().database().layouts().upsert(&saved) {
                Ok(layout) => Response::ok(id, json!({"layout": layout_to_json(&layout)})),
                Err(error) => failed(id, error.into()),
            }
        }
        _ => failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "The requested storage operation is not available.",
                format!("unknown storage RPC method: {method}"),
            ),
        ),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::rpc::Request;

    #[tokio::test]
    async fn storage_status_and_layout_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let state = CoreState::new(directory.path()).unwrap();
        let save = handle(
            state.clone(),
            Request {
                id: 1,
                method: "storage.layout.save".into(),
                params: json!({
                    "layoutId": "default",
                    "name": "Default",
                    "layout": {"version": 1, "root": {"type": "tabs"}},
                }),
            },
        )
        .await;
        assert!(save.result.is_some());
        let status = handle(
            state,
            Request {
                id: 2,
                method: "storage.status".into(),
                params: json!({}),
            },
        )
        .await;
        assert_eq!(status.result.unwrap()["healthy"], true);
    }
}
