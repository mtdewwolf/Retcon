//! RPC adapter for approvals and permission rules.

use retcon_permissions::{
    ApprovalDecision, PermissionError, RememberScope, RuleEffect,
};
use retcon_storage::{Approval, NewPermissionRule, PermissionRule};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::{Request, Response};
use crate::state::CoreState;

fn failed(id: u64, error: CoreError) -> Response {
    Response::error(id, &error)
}

fn permission_error(id: u64, error: PermissionError) -> Response {
    match error {
        PermissionError::ApprovalNotFound => failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "That approval request could not be found.",
                "approval not found",
            ),
        ),
        PermissionError::ApprovalAlreadyDecided => failed(
            id,
            CoreError::new(
                ErrorCode::InvalidRequest,
                ErrorSource::Rpc,
                "That approval request has already been decided.",
                "approval already decided",
            ),
        ),
        PermissionError::RuleNotFound => failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "That permission rule could not be found.",
                "permission rule not found",
            ),
        ),
        PermissionError::Storage(error) => failed(id, error.into()),
    }
}

fn approval_to_json(approval: &Approval) -> Value {
    json!({
        "id": approval.id,
        "sessionId": approval.session_id,
        "toolCallId": approval.tool_call_id,
        "status": approval.status,
        "request": approval.request,
        "decision": approval.decision,
        "requestedAt": approval.requested_at,
        "decidedAt": approval.decided_at,
    })
}

fn rule_to_json(rule: &PermissionRule) -> Value {
    json!({
        "id": rule.id,
        "projectId": rule.project_id,
        "scope": rule.scope,
        "effect": rule.effect,
        "matcher": rule.matcher,
        "createdAt": rule.created_at,
        "expiresAt": rule.expires_at,
    })
}

fn parse_uuid_param(id: u64, params: &Value, key: &str) -> Result<Uuid, Response> {
    match params.get(key).and_then(Value::as_str) {
        Some(raw) => Uuid::parse_str(raw).map_err(|error| {
            failed(
                id,
                CoreError::new(
                    ErrorCode::InvalidRequest,
                    ErrorSource::Rpc,
                    format!("The {key} is invalid."),
                    error.to_string(),
                ),
            )
        }),
        None => Err(failed(
            id,
            CoreError::new(
                ErrorCode::InvalidRequest,
                ErrorSource::Rpc,
                format!("The request is missing {key}."),
                format!("missing '{key}' parameter"),
            ),
        )),
    }
}

fn emit_audit(state: &CoreState, records: impl IntoIterator<Item = retcon_permissions::AuditRecord>) {
    for record in records {
        state.emit(&record.kind, record.payload);
    }
}

pub async fn handle(state: CoreState, request: Request) -> Response {
    let Request { id, method, params } = request;
    let engine = state.permissions();
    match method.as_str() {
        "approval.list" => {
            let status = params.get("status").and_then(Value::as_str);
            let session_id = match params.get("sessionId") {
                Some(value) if value.is_null() => None,
                Some(value) => match value.as_str() {
                    Some(raw) => match Uuid::parse_str(raw) {
                        Ok(parsed) => Some(parsed),
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
                                "The session ID must be a string.",
                                "sessionId must be a UUID string or null",
                            ),
                        );
                    }
                },
                None => None,
            };
            let limit = params
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(100) as usize;
            match engine.list_approvals(status, session_id, limit) {
                Ok(approvals) => Response::ok(
                    id,
                    json!({
                        "approvals": approvals.iter().map(approval_to_json).collect::<Vec<_>>(),
                        "pendingCount": engine.pending_count().unwrap_or(0),
                    }),
                ),
                Err(error) => permission_error(id, error),
            }
        }
        "approval.decide" => {
            let approval_id = match parse_uuid_param(id, &params, "approvalId") {
                Ok(value) => value,
                Err(response) => return response,
            };
            let decision = match params
                .get("decision")
                .and_then(Value::as_str)
                .and_then(ApprovalDecision::parse)
            {
                Some(decision) => decision,
                None => {
                    return failed(
                        id,
                        CoreError::new(
                            ErrorCode::InvalidRequest,
                            ErrorSource::Rpc,
                            "The approval decision must be approve or deny.",
                            "decision must be approve or deny",
                        ),
                    );
                }
            };
            let remember = RememberScope::parse(params.get("remember").and_then(Value::as_str))
                .unwrap_or(RememberScope::Once);
            let project_id = match params.get("projectId") {
                Some(value) if value.is_null() => None,
                Some(value) => match value.as_str() {
                    Some(raw) => match Uuid::parse_str(raw) {
                        Ok(parsed) => Some(parsed),
                        Err(error) => {
                            return failed(
                                id,
                                CoreError::new(
                                    ErrorCode::InvalidRequest,
                                    ErrorSource::Rpc,
                                    "The project ID is invalid.",
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
                                "The project ID must be a string.",
                                "projectId must be a UUID string or null",
                            ),
                        );
                    }
                },
                None => None,
            };
            match engine.decide(approval_id, decision, remember, project_id) {
                Ok((approval, audit)) => {
                    emit_audit(&state, audit);
                    Response::ok(id, json!({"approval": approval_to_json(&approval)}))
                }
                Err(error) => permission_error(id, error),
            }
        }
        "permission.rules.list" => {
            let project_id = match parse_uuid_param(id, &params, "projectId") {
                Ok(value) => value,
                Err(response) => return response,
            };
            match engine.list_rules(project_id) {
                Ok(rules) => Response::ok(
                    id,
                    json!({"rules": rules.iter().map(rule_to_json).collect::<Vec<_>>()}),
                ),
                Err(error) => permission_error(id, error),
            }
        }
        "permission.rules.create" => {
            let project_id = match params.get("projectId") {
                Some(value) if value.is_null() => None,
                Some(value) => match value.as_str() {
                    Some(raw) => match Uuid::parse_str(raw) {
                        Ok(parsed) => Some(parsed),
                        Err(error) => {
                            return failed(
                                id,
                                CoreError::new(
                                    ErrorCode::InvalidRequest,
                                    ErrorSource::Rpc,
                                    "The project ID is invalid.",
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
                                "The project ID must be a string.",
                                "projectId must be a UUID string or null",
                            ),
                        );
                    }
                },
                None => None,
            };
            let scope = params
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("rpc");
            let effect = match params.get("effect").and_then(Value::as_str) {
                Some(raw) => raw,
                None => {
                    return failed(
                        id,
                        CoreError::new(
                            ErrorCode::InvalidRequest,
                            ErrorSource::Rpc,
                            "The permission rule is missing an effect.",
                            "missing 'effect' parameter",
                        ),
                    );
                }
            };
            if RuleEffect::parse(effect).is_none() {
                return failed(
                    id,
                    CoreError::new(
                        ErrorCode::InvalidRequest,
                        ErrorSource::Rpc,
                        "The permission rule effect must be allow or deny.",
                        "effect must be allow or deny",
                    ),
                );
            }
            let matcher = match params.get("matcher") {
                Some(value) => value.clone(),
                None => {
                    return failed(
                        id,
                        CoreError::new(
                            ErrorCode::InvalidRequest,
                            ErrorSource::Rpc,
                            "The permission rule is missing a matcher.",
                            "missing 'matcher' parameter",
                        ),
                    );
                }
            };
            let expires_at = params.get("expiresAt").and_then(Value::as_i64);
            let rule = NewPermissionRule {
                id: Uuid::new_v4(),
                project_id,
                scope: scope.into(),
                effect: effect.into(),
                matcher,
                expires_at,
            };
            match engine.create_rule(&rule) {
                Ok((created, audit)) => {
                    emit_audit(&state, [audit]);
                    Response::ok(id, json!({"rule": rule_to_json(&created)}))
                }
                Err(error) => permission_error(id, error),
            }
        }
        "permission.rules.delete" => {
            let rule_id = match parse_uuid_param(id, &params, "ruleId") {
                Ok(value) => value,
                Err(response) => return response,
            };
            match engine.delete_rule(rule_id) {
                Ok(true) => {
                    emit_audit(
                        &state,
                        [retcon_permissions::AuditRecord::permission_rule_deleted(rule_id)],
                    );
                    Response::ok(id, json!({"deleted": true}))
                }
                Ok(false) => permission_error(id, PermissionError::RuleNotFound),
                Err(error) => permission_error(id, error),
            }
        }
        _ => failed(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "The requested permission operation is not available.",
                format!("unknown permission RPC method: {method}"),
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
    async fn approval_list_and_decide_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let state = CoreState::new(directory.path()).unwrap();
        let blocked = state
            .permissions()
            .check_rpc("agent.start", &json!({"cwd": "/tmp"}), None);
        assert!(!blocked.permission.is_allowed());
        state.emit("test", json!({}));
        for record in blocked.audit {
            state.emit(&record.kind, record.payload);
        }

        let list = handle(
            state.clone(),
            Request {
                id: 1,
                method: "approval.list".into(),
                params: json!({"status": "pending"}),
            },
        )
        .await;
        let approvals = list.result.unwrap()["approvals"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(approvals.len(), 1);

        let approval_id = approvals[0]["id"].as_str().unwrap();
        let decide = handle(
            state.clone(),
            Request {
                id: 2,
                method: "approval.decide".into(),
                params: json!({"approvalId": approval_id, "decision": "approve"}),
            },
        )
        .await;
        assert!(decide.result.is_some());
        assert_eq!(
            decide.result.unwrap()["approval"]["status"].as_str().unwrap(),
            "approved"
        );
    }
}
