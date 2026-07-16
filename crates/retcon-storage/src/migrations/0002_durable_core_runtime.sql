ALTER TABLE agent_events ADD COLUMN event_id TEXT;
ALTER TABLE agent_events ADD COLUMN category TEXT NOT NULL DEFAULT 'system';
CREATE UNIQUE INDEX idx_agent_events_event_id ON agent_events(event_id) WHERE event_id IS NOT NULL;

CREATE TABLE background_jobs (
    id TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    finished_at INTEGER,
    attempts INTEGER NOT NULL,
    max_attempts INTEGER NOT NULL,
    timeout_ms INTEGER NOT NULL,
    failure_class TEXT,
    failure TEXT,
    child_process_ids_json TEXT NOT NULL DEFAULT '[]'
) STRICT;

CREATE INDEX idx_background_jobs_status ON background_jobs(status);
