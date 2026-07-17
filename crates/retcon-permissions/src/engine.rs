//! Approval engine backed by SQLite approvals and permission rules.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use retcon_storage::{
    Approval, Database, NewApproval, NewPermissionRule, NewProject, NewSession, PermissionRule,
};

use crate::error::{PermissionError, Result};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::audit::AuditRecord;
use crate::category::ApprovalCategory;
use crate::decision::{ApprovalDecision, RememberScope, RuleEffect};
use crate::methods::{category_for_method, method_summary, requires_approval};
use crate::rules::evaluate_rules;
use crate::RpcPermission;

/// Well-known project/session used for RPC approvals without an explicit session id.
pub const SYSTEM_PROJECT_ID: Uuid = uuid::uuid!("00000000-0000-0000-0000-000000000001");
pub const SYSTEM_SESSION_ID: Uuid = uuid::uuid!("00000000-0000-0000-0000-000000000002");

/// Result of evaluating an RPC method against rules and approvals.
#[derive(Debug, Clone)]
pub struct PermissionCheck {
    pub permission: RpcPermission,
    pub audit: Vec<AuditRecord>,
}

/// Full approval engine with categories, decisions, rules, and audit emission.
#[derive(Clone)]
pub struct ApprovalEngine {
    database: Database,
    bypass: bool,
}

impl ApprovalEngine {
    #[must_use]
    pub fn new(database: Database, bypass: bool) -> Self {
        Self { database, bypass }
    }

    #[must_use]
    pub fn database(&self) -> &Database {
        &self.database
    }

    /// Evaluate permission for an RPC method using the configured bypass flag.
    pub fn check_rpc(
        &self,
        method: &str,
        params: &Value,
        project_id: Option<Uuid>,
    ) -> PermissionCheck {
        if self.bypass {
            return PermissionCheck {
                permission: RpcPermission::Allowed,
                audit: Vec::new(),
            };
        }
        if !requires_approval(method) {
            return PermissionCheck {
                permission: RpcPermission::Allowed,
                audit: Vec::new(),
            };
        }

        let now = now_ms();
        let rules = self.load_rules(project_id);
        if evaluate_rules(&rules, method, project_id, now) == Some(RuleEffect::Deny) {
            return PermissionCheck {
                permission: RpcPermission::Denied {
                    user_message: format!(
                        "Retcon blocked {method} because a permission rule denied it."
                    ),
                    technical_message: format!("permission denied for {method}: matching deny rule"),
                },
                audit: Vec::new(),
            };
        }
        if evaluate_rules(&rules, method, project_id, now) == Some(RuleEffect::Allow) {
            return PermissionCheck {
                permission: RpcPermission::Allowed,
                audit: Vec::new(),
            };
        }

        if let Some(approval_id) = params.get("approvalId").and_then(Value::as_str) {
            if let Ok(id) = Uuid::parse_str(approval_id) {
                match self.database.approvals().get(id) {
                    Ok(Some(approval)) if approval.status == "approved" && approval_matches(&approval, method, params) => {
                        return PermissionCheck {
                            permission: RpcPermission::Allowed,
                            audit: Vec::new(),
                        };
                    }
                    Ok(Some(_)) | Ok(None) => {}
                    Err(error) => {
                        return PermissionCheck {
                            permission: RpcPermission::Denied {
                                user_message: format!(
                                    "Retcon could not verify approval for {method}."
                                ),
                                technical_message: error.to_string(),
                            },
                            audit: Vec::new(),
                        };
                    }
                }
            }
        }

        let fingerprint = request_fingerprint(method, params);
        if let Ok(Some(existing)) = self.database.approvals().find_pending_by_fingerprint(&fingerprint) {
            return PermissionCheck {
                permission: denied_pending(method, existing.id),
                audit: Vec::new(),
            };
        }

        let session_id = extract_session_id(params).unwrap_or_else(|| {
            self.ensure_system_session()
                .unwrap_or(SYSTEM_SESSION_ID)
        });
        let category = category_for_method(method).unwrap_or(ApprovalCategory::System);
        let request = approval_request(method, params, project_id, &fingerprint);
        let approval = match self.database.approvals().create(&NewApproval {
            id: Uuid::new_v4(),
            session_id,
            tool_call_id: None,
            status: "pending".into(),
            request: request.clone(),
        }) {
            Ok(approval) => approval,
            Err(error) => {
                return PermissionCheck {
                    permission: RpcPermission::Denied {
                        user_message: format!(
                            "Retcon blocked {method} because approval could not be recorded."
                        ),
                        technical_message: error.to_string(),
                    },
                    audit: Vec::new(),
                };
            }
        };

        PermissionCheck {
            permission: denied_pending(method, approval.id),
            audit: vec![AuditRecord::approval_requested(
                approval.id,
                session_id,
                method,
                category,
                &request,
            )],
        }
    }

    pub fn list_approvals(
        &self,
        status: Option<&str>,
        session_id: Option<Uuid>,
        limit: usize,
    ) -> Result<Vec<Approval>> {
        Ok(self
            .database
            .approvals()
            .list(status, session_id, limit)?)
    }

    pub fn decide(
        &self,
        approval_id: Uuid,
        decision: ApprovalDecision,
        remember: RememberScope,
        project_id: Option<Uuid>,
    ) -> Result<(Approval, Vec<AuditRecord>)> {
        let Some(approval) = self.database.approvals().get(approval_id)? else {
            return Err(PermissionError::ApprovalNotFound);
        };
        if approval.status != "pending" {
            return Err(PermissionError::ApprovalAlreadyDecided);
        }

        let method = approval
            .request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let status = match decision {
            ApprovalDecision::Approve => "approved",
            ApprovalDecision::Deny => "denied",
        };
        let decision_payload = json!({
            "decision": decision.as_str(),
            "remember": remember.as_str(),
        });
        let updated = self.database.approvals().decide(
            approval_id,
            status,
            &decision_payload,
        )?;

        let mut audit = vec![AuditRecord::approval_decided(
            approval_id,
            updated.session_id,
            method,
            decision,
            Some(remember.as_str()),
        )];

        if decision == ApprovalDecision::Approve && remember == RememberScope::Always {
            let matcher = approval
                .request
                .get("matcher")
                .cloned()
                .unwrap_or_else(|| json!({"method": method}));
            let rule = NewPermissionRule {
                id: Uuid::new_v4(),
                project_id,
                scope: "rpc".into(),
                effect: "allow".into(),
                matcher: matcher.clone(),
                expires_at: None,
            };
            let created = self.database.permission_rules().create(&rule)?;
            audit.push(AuditRecord::permission_rule_created(
                created.id,
                RuleEffect::Allow,
                &matcher,
            ));
        }

        Ok((updated, audit))
    }

    pub fn list_rules(&self, project_id: Uuid) -> Result<Vec<PermissionRule>> {
        Ok(self.database.permission_rules().list_for_project(project_id)?)
    }

    pub fn create_rule(&self, rule: &NewPermissionRule) -> Result<(PermissionRule, AuditRecord)> {
        let created = self.database.permission_rules().create(rule)?;
        let effect = RuleEffect::parse(&created.effect).unwrap_or(RuleEffect::Deny);
        let audit = AuditRecord::permission_rule_created(
            created.id,
            effect,
            &created.matcher,
        );
        Ok((created, audit))
    }

    pub fn delete_rule(&self, rule_id: Uuid) -> Result<bool> {
        Ok(self.database.permission_rules().delete(rule_id)?)
    }

    pub fn pending_count(&self) -> Result<u64> {
        Ok(self.database.approvals().count_pending()?)
    }

    fn load_rules(&self, project_id: Option<Uuid>) -> Vec<PermissionRule> {
        match project_id {
            Some(project_id) => self
                .database
                .permission_rules()
                .list_for_project(project_id)
                .unwrap_or_default(),
            None => self
                .database
                .permission_rules()
                .list_global()
                .unwrap_or_default(),
        }
    }

    fn ensure_system_session(&self) -> Result<Uuid> {
        if self.database.sessions().get(SYSTEM_SESSION_ID)?.is_some() {
            return Ok(SYSTEM_SESSION_ID);
        }
        if self.database.projects().get(SYSTEM_PROJECT_ID)?.is_none() {
            self.database.projects().create(&NewProject {
                id: SYSTEM_PROJECT_ID,
                name: "Retcon System".into(),
            })?;
        }
        self.database.sessions().create(&NewSession {
            id: SYSTEM_SESSION_ID,
            project_id: SYSTEM_PROJECT_ID,
            title: "Permission approvals".into(),
            status: "system".into(),
        })?;
        Ok(SYSTEM_SESSION_ID)
    }
}

fn denied_pending(method: &str, approval_id: Uuid) -> RpcPermission {
    RpcPermission::Denied {
        user_message: format!(
            "Retcon blocked {method} because it needs explicit approval."
        ),
        technical_message: format!(
            "permission denied for {method}: pending approval {approval_id}"
        ),
    }
}

fn approval_request(
    method: &str,
    params: &Value,
    project_id: Option<Uuid>,
    fingerprint: &str,
) -> Value {
    let scrubbed_params = retcon_secrets::scrub_json(params_without_approval_id(params));
    json!({
        "kind": "rpc",
        "method": method,
        "summary": method_summary(method),
        "category": category_for_method(method).map(ApprovalCategory::as_str),
        "params": scrubbed_params,
        "projectId": project_id,
        "fingerprint": fingerprint,
        "matcher": {"method": method},
    })
}

fn approval_matches(approval: &Approval, method: &str, params: &Value) -> bool {
    approval.request.get("method").and_then(Value::as_str) == Some(method)
        && approval
            .request
            .get("fingerprint")
            .and_then(Value::as_str)
            == Some(request_fingerprint(method, params).as_str())
}

fn params_without_approval_id(params: &Value) -> Value {
    match params {
        Value::Object(map) => {
            let mut copy = map.clone();
            copy.remove("approvalId");
            Value::Object(copy)
        }
        other => other.clone(),
    }
}

fn request_fingerprint(method: &str, params: &Value) -> String {
    let canonical = json!({
        "method": method,
        "params": params_without_approval_id(params),
    });
    let encoded = serde_json::to_string(&canonical).unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    encoded.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn extract_session_id(params: &Value) -> Option<Uuid> {
    for key in ["sessionId", "session_id"] {
        if let Some(raw) = params.get(key).and_then(Value::as_str) {
            if let Ok(id) = Uuid::parse_str(raw) {
                return Some(id);
            }
        }
    }
    None
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use retcon_storage::Storage;

    fn engine() -> ApprovalEngine {
        let directory = tempfile::tempdir().unwrap();
        let storage = Storage::open(directory.path()).unwrap();
        ApprovalEngine::new(storage.database().clone(), false)
    }

    #[test]
    fn creates_pending_approval_for_mutating_rpc() {
        let engine = engine();
        let check = engine.check_rpc("agent.start", &json!({"cwd": "/tmp"}), None);
        assert!(!check.permission.is_allowed());
        assert_eq!(check.audit.len(), 1);
        assert_eq!(check.audit[0].kind, "approval.requested");
    }

    #[test]
    fn approved_grant_allows_retry_with_approval_id() {
        let engine = engine();
        let params = json!({"cwd": "/tmp"});
        let first = engine.check_rpc("agent.start", &params, None);
        assert!(!first.permission.is_allowed());
        let approval_id = first.audit[0].payload["id"]
            .as_str()
            .unwrap()
            .parse::<Uuid>()
            .unwrap();
        engine
            .decide(
                approval_id,
                ApprovalDecision::Approve,
                RememberScope::Once,
                None,
            )
            .unwrap();
        let retry = engine.check_rpc(
            "agent.start",
            &json!({"cwd": "/tmp", "approvalId": approval_id.to_string()}),
            None,
        );
        assert!(retry.permission.is_allowed());
    }
}
