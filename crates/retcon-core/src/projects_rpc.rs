//! RPC adapter for project-management operations.

use serde_json::json;
use uuid::Uuid;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::projects;
use crate::rpc::{Request, Response};
use crate::state::CoreState;

fn failed(id: u64, error: CoreError) -> Response {
    Response::error(id, &error)
}
fn missing(id: u64, field: &str) -> Response {
    failed(
        id,
        CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "The project request is missing required information.",
            format!("missing '{field}' parameter"),
        ),
    )
}
fn project_id(params: &serde_json::Value) -> Result<Uuid, CoreError> {
    params
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            CoreError::new(
                ErrorCode::InvalidRequest,
                ErrorSource::Rpc,
                "The project request is missing a project ID.",
                "missing 'projectId' parameter",
            )
        })
        .and_then(|value| {
            Uuid::parse_str(value).map_err(|e| {
                CoreError::new(
                    ErrorCode::InvalidRequest,
                    ErrorSource::Rpc,
                    "The project ID is invalid.",
                    e.to_string(),
                )
            })
        })
}

pub async fn handle(state: CoreState, request: Request) -> Response {
    let Request { id, method, params } = request;
    match method.as_str() {
        "project.open" => match params.get("path").and_then(serde_json::Value::as_str) {
            Some(path) => projects::open(&state, path)
                .await
                .map(|v| Response::ok(id, v))
                .unwrap_or_else(|e| failed(id, e)),
            None => missing(id, "path"),
        },
        "project.clone" => match (
            params.get("remoteUrl").and_then(serde_json::Value::as_str),
            params
                .get("destination")
                .and_then(serde_json::Value::as_str),
        ) {
            (Some(remote), Some(destination)) => projects::clone(&state, remote, destination)
                .await
                .map(|v| Response::ok(id, v))
                .unwrap_or_else(|e| failed(id, e)),
            _ => missing(id, "remoteUrl or destination"),
        },
        "project.list" => projects::list(
            &state,
            params.get("query").and_then(serde_json::Value::as_str),
        )
        .map(|v| Response::ok(id, v))
        .unwrap_or_else(|e| failed(id, e)),
        "project.inspect" => match params.get("path").and_then(serde_json::Value::as_str) {
            Some(path) => projects::inspect(path)
                .await
                .map(|v| Response::ok(id, v))
                .unwrap_or_else(|e| failed(id, e)),
            None => missing(id, "path"),
        },
        "project.updateMetadata" => match project_id(&params) {
            Ok(project_id) => projects::update_metadata(
                &state,
                project_id,
                params.get("metadata").unwrap_or(&json!({})),
            )
            .map(|v| Response::ok(id, v))
            .unwrap_or_else(|e| failed(id, e)),
            Err(e) => failed(id, e),
        },
        "project.remove" => match project_id(&params) {
            Ok(project_id) => projects::remove(&state, project_id)
                .map(|()| Response::ok(id, json!({})))
                .unwrap_or_else(|e| failed(id, e)),
            Err(e) => failed(id, e),
        },
        _ => failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "The requested project operation is not available.",
                format!("unknown RPC method: {method}"),
            ),
        ),
    }
}
