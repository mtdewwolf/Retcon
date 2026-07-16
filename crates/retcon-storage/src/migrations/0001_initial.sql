CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    archived_at INTEGER
) STRICT;

CREATE TABLE repository_locations (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    path TEXT NOT NULL UNIQUE,
    remote_url TEXT,
    created_at INTEGER NOT NULL,
    last_seen_at INTEGER
) STRICT;

CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    repository_location_id TEXT REFERENCES repository_locations(id) ON DELETE SET NULL,
    name TEXT NOT NULL,
    root_path TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(project_id, root_path)
) STRICT;

CREATE TABLE provider_installations (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    executable_path TEXT NOT NULL,
    version TEXT,
    status TEXT NOT NULL,
    detected_at INTEGER NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}'
) STRICT;

CREATE TABLE provider_accounts (
    id TEXT PRIMARY KEY,
    provider_installation_id TEXT NOT NULL REFERENCES provider_installations(id) ON DELETE CASCADE,
    external_id TEXT,
    display_name TEXT,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}'
) STRICT;

CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    workspace_id TEXT REFERENCES workspaces(id) ON DELETE SET NULL,
    provider_account_id TEXT REFERENCES provider_accounts(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    completed_at INTEGER,
    recovery_state TEXT
) STRICT;

CREATE TABLE turns (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL,
    status TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    error_json TEXT,
    UNIQUE(session_id, sequence)
) STRICT;

CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    turn_id TEXT NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL,
    role TEXT NOT NULL,
    content_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(turn_id, sequence)
) STRICT;

CREATE TABLE tool_calls (
    id TEXT PRIMARY KEY,
    turn_id TEXT NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    status TEXT NOT NULL,
    input_json TEXT NOT NULL,
    output_json TEXT,
    started_at INTEGER NOT NULL,
    completed_at INTEGER
) STRICT;

CREATE TABLE agent_events (
    id INTEGER PRIMARY KEY,
    session_id TEXT REFERENCES sessions(id) ON DELETE CASCADE,
    turn_id TEXT REFERENCES turns(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    completed_at INTEGER
) STRICT;

CREATE TABLE task_steps (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL,
    title TEXT NOT NULL,
    status TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(task_id, sequence)
) STRICT;

CREATE TABLE acceptance_criteria (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    description TEXT NOT NULL,
    status TEXT NOT NULL,
    evidence_json TEXT,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE approvals (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tool_call_id TEXT REFERENCES tool_calls(id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    request_json TEXT NOT NULL,
    decision_json TEXT,
    requested_at INTEGER NOT NULL,
    decided_at INTEGER
) STRICT;

CREATE TABLE permission_rules (
    id TEXT PRIMARY KEY,
    project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
    scope TEXT NOT NULL,
    effect TEXT NOT NULL,
    matcher_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER
) STRICT;

CREATE TABLE terminal_sessions (
    id TEXT PRIMARY KEY,
    session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    shell TEXT NOT NULL,
    cwd TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    log_artifact_hash TEXT
) STRICT;

CREATE TABLE commands (
    id TEXT PRIMARY KEY,
    terminal_session_id TEXT NOT NULL REFERENCES terminal_sessions(id) ON DELETE CASCADE,
    command TEXT NOT NULL,
    cwd TEXT NOT NULL,
    exit_code INTEGER,
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    output_artifact_hash TEXT
) STRICT;

CREATE TABLE git_worktrees (
    id TEXT PRIMARY KEY,
    session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    repository_location_id TEXT NOT NULL REFERENCES repository_locations(id) ON DELETE CASCADE,
    path TEXT NOT NULL UNIQUE,
    branch TEXT,
    head_oid TEXT,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    removed_at INTEGER
) STRICT;

CREATE TABLE git_checkpoints (
    id TEXT PRIMARY KEY,
    git_worktree_id TEXT NOT NULL REFERENCES git_worktrees(id) ON DELETE CASCADE,
    turn_id TEXT REFERENCES turns(id) ON DELETE SET NULL,
    kind TEXT NOT NULL,
    base_oid TEXT,
    patch_artifact_hash TEXT,
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE file_changes (
    id TEXT PRIMARY KEY,
    turn_id TEXT REFERENCES turns(id) ON DELETE CASCADE,
    git_checkpoint_id TEXT REFERENCES git_checkpoints(id) ON DELETE SET NULL,
    path TEXT NOT NULL,
    change_kind TEXT NOT NULL,
    before_artifact_hash TEXT,
    after_artifact_hash TEXT,
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE browser_sessions (
    id TEXT PRIMARY KEY,
    session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    metadata_json TEXT NOT NULL DEFAULT '{}'
) STRICT;

CREATE TABLE browser_events (
    id INTEGER PRIMARY KEY,
    browser_session_id TEXT NOT NULL REFERENCES browser_sessions(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE screenshots (
    id TEXT PRIMARY KEY,
    browser_session_id TEXT NOT NULL REFERENCES browser_sessions(id) ON DELETE CASCADE,
    artifact_hash TEXT NOT NULL,
    url TEXT,
    width INTEGER,
    height INTEGER,
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE test_runs (
    id TEXT PRIMARY KEY,
    session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    command_id TEXT REFERENCES commands(id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    summary_json TEXT NOT NULL,
    report_artifact_hash TEXT,
    started_at INTEGER NOT NULL,
    completed_at INTEGER
) STRICT;

CREATE TABLE diagnostics (
    id TEXT PRIMARY KEY,
    session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    severity TEXT NOT NULL,
    source TEXT NOT NULL,
    message TEXT NOT NULL,
    details_json TEXT,
    artifact_hash TEXT,
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE usage_records (
    id TEXT PRIMARY KEY,
    session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    provider_account_id TEXT REFERENCES provider_accounts(id) ON DELETE SET NULL,
    metric TEXT NOT NULL,
    quantity REAL NOT NULL,
    unit TEXT NOT NULL,
    recorded_at INTEGER NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}'
) STRICT;

CREATE TABLE layouts (
    id TEXT PRIMARY KEY,
    workspace_id TEXT REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    layout_json TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 0 CHECK(is_active IN (0, 1)),
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE settings (
    scope TEXT NOT NULL,
    key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(scope, key)
) WITHOUT ROWID, STRICT;

CREATE TABLE project_memories (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    content TEXT NOT NULL,
    source_json TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_sessions_project_updated ON sessions(project_id, updated_at DESC);
CREATE INDEX idx_turns_session_sequence ON turns(session_id, sequence);
CREATE INDEX idx_agent_events_session_created ON agent_events(session_id, created_at);
CREATE INDEX idx_tasks_session_status ON tasks(session_id, status);
CREATE INDEX idx_approvals_session_status ON approvals(session_id, status);
CREATE INDEX idx_commands_terminal_started ON commands(terminal_session_id, started_at);
CREATE INDEX idx_file_changes_turn_path ON file_changes(turn_id, path);
CREATE INDEX idx_browser_events_session_created ON browser_events(browser_session_id, created_at);
CREATE INDEX idx_diagnostics_session_created ON diagnostics(session_id, created_at DESC);
CREATE INDEX idx_usage_records_session_recorded ON usage_records(session_id, recorded_at);
