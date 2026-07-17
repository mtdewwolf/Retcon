//! RPC method classification for the approval middleware.

use crate::category::ApprovalCategory;

/// Returns `true` when the RPC method performs a mutating operation that requires approval.
#[must_use]
pub fn requires_approval(method: &str) -> bool {
    category_for_method(method).is_some()
}

/// Map an RPC method to its approval category when it requires approval.
#[must_use]
pub fn category_for_method(method: &str) -> Option<ApprovalCategory> {
    match method {
        "agent.start" | "agent.cancel" => Some(ApprovalCategory::Agent),
        "session.start" | "session.cancel" | "session.pause" | "session.resume" => {
            Some(ApprovalCategory::Session)
        }
        "turn.send" | "turn.cancel" => Some(ApprovalCategory::Turn),
        "git.branchCreate"
            | "git.branchDelete"
            | "git.checkout"
            | "git.worktreeAdd"
            | "git.worktreeRemove"
            | "git.worktreeAssign"
            | "git.stage"
            | "git.unstage"
            | "git.stageHunk"
            | "git.discardHunk"
            | "git.commit"
            | "git.push" => Some(ApprovalCategory::Git),
        "terminal.start" | "terminal.input" | "verification.start" | "verification.rerun" => {
            Some(ApprovalCategory::Terminal)
        }
        "file.write" => Some(ApprovalCategory::File),
        "task.acceptance.override" | "task.acceptance.delete" => {
            Some(ApprovalCategory::System)
        }
        _ => None,
    }
}

/// Short human-readable summary for an RPC approval request.
#[must_use]
pub fn method_summary(method: &str) -> &'static str {
    match method {
        "agent.start" => "Start an agent process",
        "agent.cancel" => "Cancel the running agent",
        "session.start" => "Start the agent session",
        "session.cancel" => "Cancel the agent session",
        "session.pause" => "Pause the agent session",
        "session.resume" => "Resume the agent session",
        "turn.send" => "Send a conversation turn",
        "turn.cancel" => "Cancel the active turn",
        "git.branchCreate" => "Create a Git branch",
        "git.branchDelete" => "Delete a Git branch",
        "git.checkout" => "Check out a Git branch",
        "git.worktreeAdd" => "Add a Git worktree",
        "git.worktreeRemove" => "Remove a Git worktree",
        "git.worktreeAssign" => "Assign a Git worktree to a session",
        "git.stage" => "Stage file changes",
        "git.unstage" => "Unstage file changes",
        "git.stageHunk" => "Stage a diff hunk",
        "git.discardHunk" => "Discard a diff hunk",
        "git.commit" => "Create a Git commit",
        "git.push" => "Push commits to a remote",
        "terminal.start" => "Start a terminal session",
        "terminal.input" => "Send terminal input",
        "file.write" => "Write a file",
        "task.acceptance.override" => "Override a task acceptance criterion",
        "task.acceptance.delete" => "Delete a task acceptance criterion",
        "verification.start" => "Start project verification commands",
        "verification.rerun" => "Rerun project verification commands",
        _ => "Perform a protected operation",
    }
}
