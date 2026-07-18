//! Permission rules and the approval engine.

#![allow(missing_docs)] // Phase 19 API; public documentation lands with the generated protocol.

mod audit;
mod category;
mod decision;
mod engine;
mod error;
mod methods;
mod rules;

pub use audit::AuditRecord;
pub use category::ApprovalCategory;
pub use decision::{ApprovalDecision, RememberScope, RuleEffect};
pub use engine::{ApprovalEngine, PermissionCheck, SYSTEM_PROJECT_ID, SYSTEM_SESSION_ID};
pub use error::PermissionError;
pub use methods::{category_for_method, method_summary, requires_approval};

/// Outcome of a permission check for an RPC method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcPermission {
    /// The method may proceed.
    Allowed,
    /// The method is blocked pending explicit approval.
    Denied {
        /// User-facing explanation.
        user_message: String,
        /// Technical detail for logs and support bundles.
        technical_message: String,
    },
}

impl RpcPermission {
    /// Returns `true` when the RPC method may proceed.
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }
}

/// Returns whether the development bypass env var is enabled.
#[must_use]
pub fn dev_bypass_enabled() -> bool {
    std::env::var("RETCON_PERMISSIONS_BYPASS")
        .map(|value| value == "1")
        .unwrap_or(false)
}

/// Evaluate permission for an RPC method using the process environment bypass flag.
///
/// Prefer [`ApprovalEngine::check_rpc`] in production code paths where durable
/// approvals and rules should apply.
#[must_use]
pub fn check_rpc_method(method: &str) -> RpcPermission {
    check_rpc_method_with_bypass(method, dev_bypass_enabled())
}

/// Evaluate permission for an RPC method with an explicit bypass flag (for tests).
#[must_use]
pub fn check_rpc_method_with_bypass(method: &str, bypass: bool) -> RpcPermission {
    if bypass {
        return RpcPermission::Allowed;
    }

    if requires_approval(method) {
        return RpcPermission::Denied {
            user_message: format!("Retcon blocked {method} because it needs explicit approval."),
            technical_message: format!(
                "permission denied for {method}: approval engine unavailable (set RETCON_PERMISSIONS_BYPASS=1 for dev)"
            ),
        };
    }

    RpcPermission::Allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_allows_mutating_methods() {
        for method in [
            "agent.start",
            "agent.cancel",
            "session.start",
            "session.cancel",
            "session.pause",
            "session.resume",
            "turn.send",
            "turn.cancel",
            "git.branchCreate",
            "git.worktreeAdd",
            "git.worktreeRemove",
            "terminal.start",
            "terminal.input",
            "file.write",
            "ide.openProject",
            "ide.openWorktree",
            "ide.openFile",
            "ide.openDiff",
            "ide.openTerminalLocation",
            "ide.configuration.update",
            "task.acceptance.override",
            "task.acceptance.delete",
            "verification.start",
            "verification.rerun",
            "verification.commands.configure",
            "devServer.start",
            "devServer.stop",
            "devServer.restart",
            "devServer.configure",
            "devServer.autoStart.set",
            "browser.session.start",
            "browser.session.stop",
            "browser.navigate",
            "browser.automation.script",
            "browser.automation.upload",
            "browser.takeover.start",
        ] {
            assert_eq!(
                check_rpc_method_with_bypass(method, true),
                RpcPermission::Allowed,
                "{method} should be allowed when bypass is enabled"
            );
        }
    }

    #[test]
    fn deny_mutating_agent_methods_by_default() {
        for method in ["agent.start", "agent.cancel"] {
            let decision = check_rpc_method_with_bypass(method, false);
            assert!(
                matches!(decision, RpcPermission::Denied { .. }),
                "{method} should be denied"
            );
        }
    }

    #[test]
    fn allow_read_only_agent_methods_by_default() {
        assert_eq!(
            check_rpc_method_with_bypass("agent.detect", false),
            RpcPermission::Allowed
        );
    }

    #[test]
    fn deny_git_write_ops_by_default() {
        for method in [
            "git.branchCreate",
            "git.branchDelete",
            "git.checkout",
            "git.worktreeAdd",
            "git.worktreeRemove",
            "git.worktreeAssign",
            "git.stage",
            "git.unstage",
            "git.stageHunk",
            "git.discardHunk",
            "git.commit",
            "git.push",
        ] {
            let decision = check_rpc_method_with_bypass(method, false);
            assert!(
                matches!(decision, RpcPermission::Denied { .. }),
                "{method} should be denied"
            );
        }
    }

    #[test]
    fn allow_git_read_ops_by_default() {
        for method in [
            "git.status",
            "git.defaultBranch",
            "git.branchList",
            "git.conflicts",
            "git.submodules",
            "git.worktreeList",
            "git.worktreeListStored",
            "git.diff",
        ] {
            assert_eq!(
                check_rpc_method_with_bypass(method, false),
                RpcPermission::Allowed,
                "{method} should be allowed"
            );
        }
    }

    #[test]
    fn deny_terminal_start_and_input_by_default() {
        for method in [
            "terminal.start",
            "terminal.input",
            "verification.start",
            "verification.rerun",
        ] {
            let decision = check_rpc_method_with_bypass(method, false);
            assert!(
                matches!(decision, RpcPermission::Denied { .. }),
                "{method} should be denied"
            );
        }
    }

    #[test]
    fn allow_other_terminal_ops_by_default() {
        for method in ["terminal.detectShells", "terminal.resize", "terminal.kill"] {
            assert_eq!(
                check_rpc_method_with_bypass(method, false),
                RpcPermission::Allowed,
                "{method} should be allowed"
            );
        }
    }

    #[test]
    fn deny_session_and_turn_mutations_by_default() {
        for method in [
            "session.start",
            "session.cancel",
            "session.pause",
            "session.resume",
            "turn.send",
            "turn.cancel",
        ] {
            let decision = check_rpc_method_with_bypass(method, false);
            assert!(
                matches!(decision, RpcPermission::Denied { .. }),
                "{method} should be denied"
            );
        }
    }

    #[test]
    fn allow_session_read_ops_by_default() {
        for method in ["session.create", "session.list"] {
            assert_eq!(
                check_rpc_method_with_bypass(method, false),
                RpcPermission::Allowed,
                "{method} should be allowed"
            );
        }
    }

    #[test]
    fn deny_file_write_by_default() {
        let decision = check_rpc_method_with_bypass("file.write", false);
        assert!(matches!(decision, RpcPermission::Denied { .. }));
    }

    #[test]
    fn protect_ide_launches_and_configuration_changes() {
        for method in [
            "ide.openProject",
            "ide.openWorktree",
            "ide.openFile",
            "ide.openDiff",
            "ide.openTerminalLocation",
            "ide.configuration.update",
        ] {
            assert!(matches!(
                check_rpc_method_with_bypass(method, false),
                RpcPermission::Denied { .. }
            ));
        }
        for method in ["ide.detect", "ide.configuration.get"] {
            assert_eq!(
                check_rpc_method_with_bypass(method, false),
                RpcPermission::Allowed
            );
        }
    }

    #[test]
    fn deny_acceptance_override_and_delete_by_default() {
        for method in ["task.acceptance.override", "task.acceptance.delete"] {
            assert!(matches!(
                check_rpc_method_with_bypass(method, false),
                RpcPermission::Denied { .. }
            ));
        }
    }

    #[test]
    fn deny_verification_requirement_changes_by_default() {
        assert!(matches!(
            check_rpc_method_with_bypass("verification.commands.configure", false),
            RpcPermission::Denied { .. }
        ));
    }

    #[test]
    fn deny_development_server_process_and_environment_changes_by_default() {
        for method in [
            "devServer.start",
            "devServer.stop",
            "devServer.restart",
            "devServer.configure",
            "devServer.autoStart.set",
        ] {
            assert!(matches!(
                check_rpc_method_with_bypass(method, false),
                RpcPermission::Denied { .. }
            ));
        }
    }

    #[test]
    fn deny_browser_lifecycle_navigation_automation_and_takeover_by_default() {
        for method in [
            "browser.session.start",
            "browser.session.stop",
            "browser.tab.open",
            "browser.navigate",
            "browser.observation.screenshot",
            "browser.automation.action",
            "browser.automation.script",
            "browser.automation.upload",
            "browser.automation.download",
            "browser.takeover.start",
            "browser.takeover.stop",
        ] {
            assert!(matches!(
                check_rpc_method_with_bypass(method, false),
                RpcPermission::Denied { .. }
            ));
        }
    }

    #[test]
    fn allow_file_reads_by_default() {
        for method in ["file.list", "file.read", "file.watch", "file.unwatch"] {
            assert_eq!(
                check_rpc_method_with_bypass(method, false),
                RpcPermission::Allowed,
                "{method} should be allowed"
            );
        }
    }

    #[test]
    fn unrelated_methods_are_allowed() {
        assert_eq!(
            check_rpc_method_with_bypass("core.health", false),
            RpcPermission::Allowed
        );
    }
}
