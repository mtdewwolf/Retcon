-- Phase 24: durable managed-browser ownership, tabs, evidence, and takeover history.

CREATE TABLE browser_profiles (
    id BLOB PRIMARY KEY,
    project_id BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    worktree_id BLOB REFERENCES git_worktrees(id) ON DELETE SET NULL,
    task_id BLOB REFERENCES tasks(id) ON DELETE SET NULL,
    path TEXT NOT NULL UNIQUE,
    persistent INTEGER NOT NULL DEFAULT 0 CHECK(persistent IN (0,1)),
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    released_at INTEGER
) STRICT;

ALTER TABLE browser_sessions ADD COLUMN project_id BLOB REFERENCES projects(id) ON DELETE CASCADE;
ALTER TABLE browser_sessions ADD COLUMN task_id BLOB REFERENCES tasks(id) ON DELETE SET NULL;
ALTER TABLE browser_sessions ADD COLUMN worktree_id BLOB REFERENCES git_worktrees(id) ON DELETE SET NULL;
ALTER TABLE browser_sessions ADD COLUMN dev_server_instance_id BLOB REFERENCES dev_server_instances(id) ON DELETE SET NULL;
ALTER TABLE browser_sessions ADD COLUMN profile_id BLOB REFERENCES browser_profiles(id) ON DELETE SET NULL;
ALTER TABLE browser_sessions ADD COLUMN service_session_id TEXT;
ALTER TABLE browser_sessions ADD COLUMN service_version TEXT;
ALTER TABLE browser_sessions ADD COLUMN service_protocol INTEGER;
ALTER TABLE browser_sessions ADD COLUMN network_policy TEXT NOT NULL DEFAULT 'loopback';
ALTER TABLE browser_sessions ADD COLUMN failure TEXT;
ALTER TABLE browser_sessions ADD COLUMN updated_at INTEGER;

CREATE TABLE browser_tabs (
    id BLOB PRIMARY KEY,
    browser_session_id BLOB NOT NULL REFERENCES browser_sessions(id) ON DELETE CASCADE,
    service_tab_id TEXT NOT NULL,
    url TEXT,
    title TEXT,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    closed_at INTEGER,
    UNIQUE(browser_session_id, service_tab_id)
) STRICT;

CREATE TABLE browser_takeovers (
    id BLOB PRIMARY KEY,
    browser_session_id BLOB NOT NULL REFERENCES browser_sessions(id) ON DELETE CASCADE,
    actor TEXT NOT NULL,
    reason TEXT,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    end_reason TEXT
) STRICT;

CREATE TABLE browser_observations (
    id BLOB PRIMARY KEY,
    browser_session_id BLOB NOT NULL REFERENCES browser_sessions(id) ON DELETE CASCADE,
    tab_id BLOB REFERENCES browser_tabs(id) ON DELETE SET NULL,
    kind TEXT NOT NULL,
    artifact_hash TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK(size_bytes >= 0),
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE browser_history (
    id INTEGER PRIMARY KEY,
    browser_session_id BLOB NOT NULL,
    project_id BLOB NOT NULL,
    kind TEXT NOT NULL,
    actor TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE browser_console_entries (
    id INTEGER PRIMARY KEY,
    browser_session_id BLOB NOT NULL REFERENCES browser_sessions(id) ON DELETE CASCADE,
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE browser_network_entries (
    id INTEGER PRIMARY KEY,
    browser_session_id BLOB NOT NULL REFERENCES browser_sessions(id) ON DELETE CASCADE,
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;

CREATE UNIQUE INDEX idx_browser_active_profile_owner
    ON browser_profiles(project_id,ifnull(hex(worktree_id),''),ifnull(hex(task_id),''))
    WHERE status='active';
CREATE INDEX idx_browser_sessions_project ON browser_sessions(project_id,status,started_at DESC);
CREATE INDEX idx_browser_sessions_task ON browser_sessions(task_id,started_at DESC);
CREATE INDEX idx_browser_tabs_session ON browser_tabs(browser_session_id,status,created_at);
CREATE UNIQUE INDEX idx_browser_active_takeover
    ON browser_takeovers(browser_session_id) WHERE ended_at IS NULL;
CREATE INDEX idx_browser_observations_session ON browser_observations(browser_session_id,created_at DESC);
CREATE INDEX idx_browser_history_session ON browser_history(browser_session_id,id);
CREATE INDEX idx_browser_console_session ON browser_console_entries(browser_session_id,id);
CREATE INDEX idx_browser_network_session ON browser_network_entries(browser_session_id,id);
