//! Permission rule matching for RPC methods.

use serde_json::Value;
use uuid::Uuid;

use crate::category::ApprovalCategory;
use crate::decision::RuleEffect;
use crate::methods::category_for_method;
use retcon_storage::PermissionRule;

/// Evaluate persisted rules and return the first matching effect, if any.
pub fn evaluate_rules(
    rules: &[PermissionRule],
    method: &str,
    project_id: Option<Uuid>,
    now_ms: i64,
) -> Option<RuleEffect> {
    let category = category_for_method(method).map(ApprovalCategory::as_str);
    for rule in rules {
        if rule.expires_at.is_some_and(|expires| expires <= now_ms) {
            continue;
        }
        if let Some(project_id) = project_id {
            if rule.project_id.is_some() && rule.project_id != Some(project_id) {
                continue;
            }
        } else if rule.project_id.is_some() {
            continue;
        }
        if rule.scope != "rpc" && rule.scope != "global" {
            continue;
        }
        if matcher_matches(&rule.matcher, method, category) {
            return RuleEffect::parse(&rule.effect);
        }
    }
    None
}

fn matcher_matches(matcher: &Value, method: &str, category: Option<&str>) -> bool {
    if let Some(exact) = matcher.get("method").and_then(Value::as_str)
        && exact == method
    {
        return true;
    }
    if let Some(methods) = matcher.get("methods").and_then(Value::as_array) {
        for pattern in methods {
            if let Some(pattern) = pattern.as_str()
                && method_matches_pattern(method, pattern)
            {
                return true;
            }
        }
    }
    if let Some(expected) = matcher.get("category").and_then(Value::as_str)
        && category == Some(expected)
    {
        return true;
    }
    false
}

fn method_matches_pattern(method: &str, pattern: &str) -> bool {
    if pattern == method {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix(".*") {
        return method.starts_with(prefix) && method.as_bytes().get(prefix.len()) == Some(&b'.');
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use retcon_storage::NewPermissionRule;

    #[test]
    fn exact_method_rule_matches() {
        let rule = PermissionRule {
            id: Uuid::new_v4(),
            project_id: None,
            scope: "rpc".into(),
            effect: "allow".into(),
            matcher: serde_json::json!({"method": "agent.start"}),
            created_at: 0,
            expires_at: None,
        };
        assert_eq!(
            evaluate_rules(&[rule], "agent.start", None, 0),
            Some(RuleEffect::Allow)
        );
    }

    #[test]
    fn wildcard_rule_matches_prefix() {
        let created = NewPermissionRule::new(
            "rpc",
            "deny",
            serde_json::json!({"methods": ["git.*"]}),
        );
        let rule = PermissionRule {
            id: created.id,
            project_id: created.project_id,
            scope: created.scope,
            effect: created.effect,
            matcher: created.matcher,
            created_at: 0,
            expires_at: None,
        };
        assert_eq!(
            evaluate_rules(&[rule], "git.branchCreate", None, 0),
            Some(RuleEffect::Deny)
        );
    }
}
