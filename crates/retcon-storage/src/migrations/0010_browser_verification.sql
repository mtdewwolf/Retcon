-- Phase 25: durable browser verification definitions, execution evidence, and review.

CREATE TABLE browser_verification_definitions (
    id BLOB PRIMARY KEY,
    project_id BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_id BLOB REFERENCES tasks(id) ON DELETE CASCADE,
    dev_server_config_id BLOB REFERENCES dev_server_configs(id) ON DELETE SET NULL,
    name TEXT NOT NULL,
    target_url TEXT NOT NULL,
    steps_json TEXT NOT NULL DEFAULT '[]',
    assertions_json TEXT NOT NULL DEFAULT '[]',
    visual_policy_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(visual_policy_json) AND json_type(visual_policy_json)='object' AND length(visual_policy_json)<=65536),
    fail_on_accessibility INTEGER NOT NULL DEFAULT 1 CHECK(fail_on_accessibility IN (0,1)),
    status TEXT NOT NULL CHECK(status IN ('active','disabled','archived')),
    is_required INTEGER NOT NULL DEFAULT 1 CHECK(is_required IN (0,1)),
    timeout_ms INTEGER NOT NULL CHECK(timeout_ms BETWEEN 500 AND 120000),
    max_retries INTEGER NOT NULL DEFAULT 0 CHECK(max_retries BETWEEN 0 AND 3),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(project_id,name)
) STRICT;

CREATE TABLE browser_verification_variants (
    id BLOB PRIMARY KEY,
    definition_id BLOB NOT NULL REFERENCES browser_verification_definitions(id) ON DELETE CASCADE,
    variant_key TEXT NOT NULL,
    width INTEGER NOT NULL CHECK(width BETWEEN 1 AND 16384),
    height INTEGER NOT NULL CHECK(height BETWEEN 1 AND 16384),
    device_scale REAL NOT NULL DEFAULT 1.0 CHECK(device_scale > 0 AND device_scale <= 8),
    device_name TEXT,
    sort_order INTEGER NOT NULL,
    UNIQUE(definition_id,variant_key),
    UNIQUE(definition_id,sort_order)
) STRICT;

CREATE TABLE browser_verification_runs (
    id BLOB PRIMARY KEY,
    definition_id BLOB NOT NULL REFERENCES browser_verification_definitions(id) ON DELETE CASCADE,
    project_id BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_id BLOB NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    dev_server_instance_id BLOB REFERENCES dev_server_instances(id) ON DELETE SET NULL,
    browser_session_id BLOB REFERENCES browser_sessions(id) ON DELETE SET NULL,
    status TEXT NOT NULL CHECK(status IN ('queued','running','passed','failed','needs_review','approved','cancelled','interrupted','error')),
    attempt INTEGER NOT NULL DEFAULT 1 CHECK(attempt > 0),
    max_attempts INTEGER NOT NULL CHECK(max_attempts BETWEEN 1 AND 4),
    timeout_ms INTEGER NOT NULL CHECK(timeout_ms BETWEEN 500 AND 120000),
    idempotency_key TEXT,
    runner_version TEXT,
    blocking_failures INTEGER NOT NULL DEFAULT 0 CHECK(blocking_failures >= 0),
    critical_accessibility INTEGER NOT NULL DEFAULT 0 CHECK(critical_accessibility >= 0),
    warning_count INTEGER NOT NULL DEFAULT 0 CHECK(warning_count >= 0),
    visual_differences INTEGER NOT NULL DEFAULT 0 CHECK(visual_differences >= 0),
    console_errors INTEGER NOT NULL DEFAULT 0 CHECK(console_errors >= 0),
    summary_json TEXT NOT NULL DEFAULT '{}',
    review_json TEXT NOT NULL DEFAULT '{}',
    failure TEXT,
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    completed_at INTEGER,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE UNIQUE INDEX idx_browser_verification_run_idempotency
    ON browser_verification_runs(definition_id,task_id,idempotency_key)
    WHERE idempotency_key IS NOT NULL;

CREATE TABLE browser_verification_events (
    id INTEGER PRIMARY KEY,
    run_id BLOB NOT NULL REFERENCES browser_verification_runs(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK(sequence > 0),
    kind TEXT NOT NULL,
    severity TEXT NOT NULL CHECK(severity IN ('info','warning','error','critical')),
    actor TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(run_id,sequence)
) STRICT;

CREATE TABLE browser_verification_artifacts (
    id BLOB PRIMARY KEY,
    run_id BLOB NOT NULL REFERENCES browser_verification_runs(id) ON DELETE CASCADE,
    event_id INTEGER REFERENCES browser_verification_events(id) ON DELETE SET NULL,
    kind TEXT NOT NULL,
    artifact_hash TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK(size_bytes >= 0),
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE browser_verification_baselines (
    id BLOB PRIMARY KEY,
    definition_id BLOB NOT NULL REFERENCES browser_verification_definitions(id) ON DELETE CASCADE,
    variant_key TEXT NOT NULL,
    artifact_hash TEXT NOT NULL,
    source_run_id BLOB REFERENCES browser_verification_runs(id) ON DELETE SET NULL,
    status TEXT NOT NULL CHECK(status IN ('active','superseded')),
    approved_by TEXT NOT NULL,
    approved_at INTEGER NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}'
) STRICT;

CREATE UNIQUE INDEX idx_browser_verification_active_baseline
    ON browser_verification_baselines(definition_id,variant_key) WHERE status='active';

CREATE TABLE browser_visual_comparisons (
    id BLOB PRIMARY KEY,
    run_id BLOB NOT NULL REFERENCES browser_verification_runs(id) ON DELETE CASCADE,
    baseline_id BLOB REFERENCES browser_verification_baselines(id) ON DELETE SET NULL,
    variant_key TEXT NOT NULL,
    current_artifact_hash TEXT NOT NULL,
    difference_artifact_hash TEXT,
    pixel_difference_ratio REAL NOT NULL CHECK(pixel_difference_ratio >= 0 AND pixel_difference_ratio <= 1),
    perceptual_difference_ratio REAL CHECK(perceptual_difference_ratio >= 0 AND perceptual_difference_ratio <= 1),
    threshold_ratio REAL NOT NULL CHECK(threshold_ratio >= 0 AND threshold_ratio <= 1),
    status TEXT NOT NULL CHECK(status IN ('matched','different','missing_baseline','approved')),
    ignore_regions_json TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE browser_console_evidence (
    id INTEGER PRIMARY KEY,
    run_id BLOB NOT NULL REFERENCES browser_verification_runs(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL,
    level TEXT NOT NULL,
    message TEXT NOT NULL,
    source TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE(run_id,sequence)
) STRICT;

CREATE TABLE browser_network_evidence (
    id INTEGER PRIMARY KEY,
    run_id BLOB NOT NULL REFERENCES browser_verification_runs(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL,
    method TEXT NOT NULL,
    url TEXT NOT NULL,
    status_code INTEGER,
    failure TEXT,
    duration_ms INTEGER,
    created_at INTEGER NOT NULL,
    UNIQUE(run_id,sequence)
) STRICT;

CREATE TABLE browser_accessibility_findings (
    id BLOB PRIMARY KEY,
    run_id BLOB NOT NULL REFERENCES browser_verification_runs(id) ON DELETE CASCADE,
    rule_id TEXT NOT NULL,
    severity TEXT NOT NULL CHECK(severity IN ('warning','critical')),
    message TEXT NOT NULL,
    selector TEXT,
    help_url TEXT,
    status TEXT NOT NULL CHECK(status IN ('open','acknowledged','waived')),
    artifact_hash TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_browser_verification_definitions_scope
    ON browser_verification_definitions(project_id,task_id,status,name);
CREATE INDEX idx_browser_verification_runs_task
    ON browser_verification_runs(task_id,created_at DESC,id DESC);
CREATE INDEX idx_browser_verification_runs_definition
    ON browser_verification_runs(definition_id,task_id,created_at DESC,id DESC);
CREATE INDEX idx_browser_verification_events_run
    ON browser_verification_events(run_id,sequence);
CREATE INDEX idx_browser_verification_artifacts_run
    ON browser_verification_artifacts(run_id,created_at,id);
CREATE INDEX idx_browser_visual_comparisons_run
    ON browser_visual_comparisons(run_id,variant_key);
CREATE INDEX idx_browser_accessibility_run
    ON browser_accessibility_findings(run_id,severity,status);
