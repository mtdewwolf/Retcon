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
        "git.branchCreate" | "git.branchDelete" | "git.checkout" | "git.worktreeAdd"
        | "git.worktreeRemove" | "git.worktreeAssign" | "git.stage" | "git.unstage"
        | "git.stageHunk" | "git.discardHunk" | "git.commit" | "git.push" => {
            Some(ApprovalCategory::Git)
        }
        "terminal.start" | "terminal.input" | "verification.start" | "verification.rerun" => {
            Some(ApprovalCategory::Terminal)
        }
        "file.write" => Some(ApprovalCategory::File),
        "task.acceptance.override"
        | "task.acceptance.delete"
        | "verification.commands.configure" => Some(ApprovalCategory::System),
        "devServer.start" | "devServer.stop" | "devServer.restart" => {
            Some(ApprovalCategory::Terminal)
        }
        "devServer.configure" | "devServer.autoStart.set" => Some(ApprovalCategory::System),
        "browser.startService"
        | "browser.stopService"
        | "browser.call"
        | "browser.session.start"
        | "browser.session.stop"
        | "browser.tab.open"
        | "browser.tab.close"
        | "browser.tab.activate"
        | "browser.navigate"
        | "browser.back"
        | "browser.forward"
        | "browser.reload"
        | "browser.observation.screenshot"
        | "browser.observation.snapshot"
        | "browser.observation.logs"
        | "browser.observation.trace"
        | "browser.automation.action"
        | "browser.automation.script"
        | "browser.automation.upload"
        | "browser.automation.download"
        | "browser.takeover.start"
        | "browser.takeover.stop"
        | "browser.verification.definition.create"
        | "browser.verification.definition.update"
        | "browser.verification.run"
        | "browser.verification.cancel"
        | "browser.verification.review"
        | "browser.verification.baseline.approve" => Some(ApprovalCategory::Browser),
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
        "verification.commands.configure" => "Change project verification requirements",
        "devServer.start" => "Start a development server",
        "devServer.stop" => "Stop a development server",
        "devServer.restart" => "Restart a development server",
        "devServer.configure" => "Change development server settings or environment",
        "devServer.autoStart.set" => "Change development server auto-start settings",
        "browser.session.start" => "Start a managed browser session",
        "browser.startService" => "Start the legacy managed browser service",
        "browser.stopService" => "Stop the legacy managed browser service",
        "browser.call" => "Run a legacy managed browser operation",
        "browser.session.stop" => "Stop a managed browser session",
        "browser.tab.open" => "Open a managed browser tab",
        "browser.tab.close" => "Close a managed browser tab",
        "browser.tab.activate" => "Activate a managed browser tab",
        "browser.navigate" => "Navigate the managed browser",
        "browser.back" => "Navigate the managed browser backward",
        "browser.forward" => "Navigate the managed browser forward",
        "browser.reload" => "Reload the managed browser tab",
        "browser.observation.screenshot" => "Capture a browser screenshot",
        "browser.observation.snapshot" => "Capture a browser accessibility snapshot",
        "browser.observation.logs" => "Capture browser console and network logs",
        "browser.observation.trace" => "Capture a browser trace",
        "browser.automation.action" => "Run a browser automation action",
        "browser.automation.script" => "Run a script in the managed browser",
        "browser.automation.upload" => "Upload project files through the browser",
        "browser.automation.download" => "Download a browser file into the project",
        "browser.takeover.start" => "Take manual control of the managed browser",
        "browser.takeover.stop" => "Release manual control of the managed browser",
        "browser.verification.definition.create" => "Create a browser verification definition",
        "browser.verification.definition.update" => "Change a browser verification definition",
        "browser.verification.run" => "Run browser verification",
        "browser.verification.cancel" => "Cancel browser verification",
        "browser.verification.review" => "Review browser verification evidence",
        "browser.verification.baseline.approve" => "Approve a browser visual baseline",
        _ => "Perform a protected operation",
    }
}
