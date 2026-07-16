-- Partial indexes for crash-recovery scans over active rows.
CREATE INDEX idx_sessions_active_status ON sessions(status)
    WHERE status IN ('starting', 'running');
CREATE INDEX idx_turns_active_status ON turns(status)
    WHERE status IN ('queued', 'running');
CREATE INDEX idx_terminal_sessions_active_status ON terminal_sessions(status)
    WHERE status IN ('starting', 'running');
CREATE INDEX idx_browser_sessions_active_status ON browser_sessions(status)
    WHERE status IN ('starting', 'running');
CREATE INDEX idx_approvals_pending_status ON approvals(status)
    WHERE status = 'pending';
CREATE INDEX idx_tasks_active_status ON tasks(status)
    WHERE status NOT IN ('completed', 'cancelled', 'failed');
CREATE INDEX idx_background_jobs_active_status ON background_jobs(status)
    WHERE status IN ('queued', 'running', 'stuck');

-- Artifact hash lookups used by retention cleanup.
CREATE INDEX idx_terminal_sessions_log_hash ON terminal_sessions(log_artifact_hash)
    WHERE log_artifact_hash IS NOT NULL;
CREATE INDEX idx_commands_output_hash ON commands(output_artifact_hash)
    WHERE output_artifact_hash IS NOT NULL;
CREATE INDEX idx_git_checkpoints_patch_hash ON git_checkpoints(patch_artifact_hash)
    WHERE patch_artifact_hash IS NOT NULL;
CREATE INDEX idx_file_changes_before_hash ON file_changes(before_artifact_hash)
    WHERE before_artifact_hash IS NOT NULL;
CREATE INDEX idx_file_changes_after_hash ON file_changes(after_artifact_hash)
    WHERE after_artifact_hash IS NOT NULL;
CREATE INDEX idx_screenshots_artifact_hash ON screenshots(artifact_hash);
CREATE INDEX idx_test_runs_report_hash ON test_runs(report_artifact_hash)
    WHERE report_artifact_hash IS NOT NULL;
CREATE INDEX idx_diagnostics_artifact_hash ON diagnostics(artifact_hash)
    WHERE artifact_hash IS NOT NULL;

CREATE INDEX idx_background_jobs_finished_at ON background_jobs(finished_at DESC)
    WHERE finished_at IS NOT NULL;
