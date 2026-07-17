//! RPC adapter for the non-invasive Provider Doctor checks.

use retcon_agents::AgentProvider;
use serde_json::json;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::{Request, Response};

/// Handle `provider.*` diagnostic requests.
pub async fn handle(request: Request) -> Response {
    let Request { id, method, params } = request;
    match method.as_str() {
        "provider.metadata" => {
            let provider = params
                .get("providerId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("claude-code");
            if provider != retcon_agents::ClaudeCodeProvider::ID {
                return Response::error(
                    id,
                    &CoreError::new(
                        ErrorCode::NotFound,
                        ErrorSource::Rpc,
                        "That provider is not available in this Retcon build.",
                        format!("unsupported provider id: {provider}"),
                    ),
                );
            }
            Response::ok(id, json!(retcon_agents::ClaudeCodeProvider.metadata()))
        }
        "provider.doctor" => {
            let provider = params
                .get("providerId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("claude-code");
            if provider != "claude-code" {
                return Response::error(
                    id,
                    &CoreError::new(
                        ErrorCode::NotFound,
                        ErrorSource::Rpc,
                        "That provider is not available in this Retcon build.",
                        format!("unsupported provider id: {provider}"),
                    ),
                );
            }
            let force_refresh = params
                .get("forceRefresh")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            if force_refresh {
                retcon_agents::clear_detection_cache();
            }
            let report = retcon_agents::doctor_claude().await;
            let include_bundle = params
                .get("includeBundle")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            if include_bundle {
                Response::ok(id, report.diagnostic_bundle())
            } else {
                Response::ok(id, json!(report))
            }
        }
        _ => Response::error(
            id,
            &CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "The requested provider operation is not available.",
                format!("unknown RPC method: {method}"),
            ),
        ),
    }
}
