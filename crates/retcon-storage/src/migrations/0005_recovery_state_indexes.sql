-- Expand recovery partial indexes to match recover_interrupted predicates.
DROP INDEX IF EXISTS idx_sessions_active_status;
DROP INDEX IF EXISTS idx_turns_active_status;

CREATE INDEX idx_sessions_active_status ON sessions(status)
    WHERE status IN ('starting', 'running', 'waiting_for_approval', 'waiting_for_user');
CREATE INDEX idx_turns_active_status ON turns(status)
    WHERE status IN (
        'queued',
        'sending',
        'running',
        'tool_execution',
        'waiting_for_approval',
        'completing'
    );

-- Speeds global event retention deletes by created_at cutoff.
CREATE INDEX IF NOT EXISTS idx_agent_events_created_at ON agent_events(created_at);

-- Normalize pre-a1e7eb3 recovery labels to the new session/turn vocabulary.
UPDATE sessions
SET status = 'disconnected',
    recovery_state = coalesce(recovery_state, 'process_restart'),
    updated_at = CAST(strftime('%s','now') AS INTEGER) * 1000
WHERE status = 'interrupted';

UPDATE turns
SET status = 'failed',
    completed_at = coalesce(completed_at, CAST(strftime('%s','now') AS INTEGER) * 1000),
    error_json = coalesce(error_json, '{"reason":"process_restart"}')
WHERE status = 'interrupted';
