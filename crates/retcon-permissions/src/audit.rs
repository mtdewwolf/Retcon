//! Structured audit records emitted through the core event bus.

use serde_json::{Value, json};
use uuid::Uuid;

use crate::category::ApprovalCategory;
use crate::decision::{ApprovalDecision, RuleEffect};

/// A durable audit event produced by the approval engine.
#[derive(Debug, Clone)]
pub struct AuditRecord {
    pub kind: String,
    pub payload: Value,
}

impl AuditRecord {
    pub fn approval_requested(
        approval_id: Uuid,
        session_id: Uuid,
        method: &str,
        category: ApprovalCategory,
        request: &Value,
    ) -> Self {
        Self {
            kind: "approval.requested".into(),
            payload: json!({
                "id": approval_id,
                "sessionId": session_id,
                "method": method,
                "category": category.as_str(),
                "title": crate::methods::method_summary(method),
                "request": request,
            }),
        }
    }

    pub fn approval_decided(
        approval_id: Uuid,
        session_id: Uuid,
        method: &str,
        decision: ApprovalDecision,
        remember: Option<&str>,
    ) -> Self {
        Self {
            kind: "approval.decided".into(),
            payload: json!({
                "id": approval_id,
                "sessionId": session_id,
                "method": method,
                "decision": decision.as_str(),
                "remember": remember,
            }),
        }
    }

    pub fn permission_rule_created(rule_id: Uuid, effect: RuleEffect, matcher: &Value) -> Self {
        Self {
            kind: "permission.rule_created".into(),
            payload: json!({
                "id": rule_id,
                "effect": effect.as_str(),
                "matcher": matcher,
            }),
        }
    }

    pub fn permission_rule_deleted(rule_id: Uuid) -> Self {
        Self {
            kind: "permission.rule_deleted".into(),
            payload: json!({"id": rule_id}),
        }
    }
}
