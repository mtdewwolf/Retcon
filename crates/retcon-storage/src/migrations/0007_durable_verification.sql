-- Phase 22: durable verification commands, runs, gates, results, artifacts, and history.

CREATE TABLE project_verification_commands (
    id BLOB PRIMARY KEY,
    project_id BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    command_key TEXT NOT NULL,
    gate_kind TEXT NOT NULL,
    command TEXT NOT NULL,
    cwd TEXT,
    is_required INTEGER NOT NULL DEFAULT 1 CHECK(is_required IN (0, 1)),
    is_enabled INTEGER NOT NULL DEFAULT 1 CHECK(is_enabled IN (0, 1)),
    timeout_ms INTEGER CHECK(timeout_ms IS NULL OR timeout_ms > 0),
    updated_at INTEGER NOT NULL,
    UNIQUE(project_id, command_key)
) STRICT;

CREATE TABLE verification_runs (
    id BLOB PRIMARY KEY,
    task_id BLOB NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    project_id BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    rerun_of_id BLOB REFERENCES verification_runs(id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    trigger_kind TEXT NOT NULL,
    summary_json TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    completed_at INTEGER
) STRICT;

CREATE TABLE verification_gates (
    id BLOB PRIMARY KEY,
    run_id BLOB NOT NULL REFERENCES verification_runs(id) ON DELETE CASCADE,
    command_id BLOB REFERENCES project_verification_commands(id) ON DELETE SET NULL,
    command_key TEXT NOT NULL,
    gate_kind TEXT NOT NULL,
    command TEXT NOT NULL,
    cwd TEXT,
    is_required INTEGER NOT NULL CHECK(is_required IN (0, 1)),
    timeout_ms INTEGER,
    status TEXT NOT NULL,
    summary_json TEXT NOT NULL DEFAULT '{}',
    started_at INTEGER,
    completed_at INTEGER,
    UNIQUE(run_id, command_key)
) STRICT;

CREATE TABLE verification_test_results (
    id BLOB PRIMARY KEY,
    run_id BLOB NOT NULL REFERENCES verification_runs(id) ON DELETE CASCADE,
    gate_id BLOB NOT NULL REFERENCES verification_gates(id) ON DELETE CASCADE,
    suite TEXT,
    name TEXT NOT NULL,
    status TEXT NOT NULL,
    duration_ms INTEGER CHECK(duration_ms IS NULL OR duration_ms >= 0),
    file_path TEXT,
    line INTEGER CHECK(line IS NULL OR line > 0),
    message TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}'
) STRICT;

CREATE TABLE verification_artifacts (
    id BLOB PRIMARY KEY,
    run_id BLOB NOT NULL REFERENCES verification_runs(id) ON DELETE CASCADE,
    gate_id BLOB REFERENCES verification_gates(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    artifact_hash TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK(size_bytes >= 0),
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL
) STRICT;

-- Deliberately independent of run/task foreign keys so the audit trail survives deletion.
CREATE TABLE verification_events (
    id INTEGER PRIMARY KEY,
    run_id BLOB NOT NULL,
    task_id BLOB NOT NULL,
    gate_id BLOB,
    kind TEXT NOT NULL,
    actor TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_verification_commands_project
    ON project_verification_commands(project_id, gate_kind, command_key);
CREATE INDEX idx_verification_runs_task_created
    ON verification_runs(task_id, created_at DESC, id);
CREATE INDEX idx_verification_runs_project_created
    ON verification_runs(project_id, created_at DESC, id);
CREATE INDEX idx_verification_gates_run
    ON verification_gates(run_id, gate_kind, command_key);
CREATE INDEX idx_verification_results_gate
    ON verification_test_results(gate_id, status, name);
CREATE INDEX idx_verification_artifacts_run
    ON verification_artifacts(run_id, gate_id, kind);
CREATE INDEX idx_verification_events_run
    ON verification_events(run_id, created_at, id);
