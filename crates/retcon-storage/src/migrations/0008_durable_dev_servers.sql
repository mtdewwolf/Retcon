-- Phase 23: durable development-server configuration, lifecycle, logs, and ports.

CREATE TABLE dev_server_configs (
    id BLOB PRIMARY KEY,
    project_id BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    worktree_id BLOB REFERENCES git_worktrees(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    command TEXT NOT NULL,
    cwd TEXT NOT NULL,
    host TEXT NOT NULL DEFAULT '127.0.0.1',
    preferred_port INTEGER CHECK(preferred_port IS NULL OR preferred_port BETWEEN 1024 AND 65535),
    auto_start INTEGER NOT NULL DEFAULT 0 CHECK(auto_start IN (0,1)),
    env_allowlist_json TEXT NOT NULL DEFAULT '[]',
    environment_json TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(project_id, worktree_id, name)
) STRICT;

CREATE TABLE dev_server_instances (
    id BLOB PRIMARY KEY,
    config_id BLOB NOT NULL REFERENCES dev_server_configs(id) ON DELETE CASCADE,
    project_id BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    worktree_id BLOB REFERENCES git_worktrees(id) ON DELETE SET NULL,
    task_id BLOB REFERENCES tasks(id) ON DELETE SET NULL,
    port INTEGER NOT NULL CHECK(port BETWEEN 1024 AND 65535),
    status TEXT NOT NULL,
    pid INTEGER,
    url TEXT,
    preview_json TEXT NOT NULL DEFAULT '{}',
    log_artifact_hash TEXT,
    failure TEXT,
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    stopped_at INTEGER
) STRICT;

CREATE TABLE dev_server_port_leases (
    id BLOB PRIMARY KEY,
    port INTEGER NOT NULL CHECK(port BETWEEN 1024 AND 65535),
    project_id BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    worktree_id BLOB REFERENCES git_worktrees(id) ON DELETE CASCADE,
    task_id BLOB REFERENCES tasks(id) ON DELETE CASCADE,
    config_id BLOB NOT NULL REFERENCES dev_server_configs(id) ON DELETE CASCADE,
    instance_id BLOB REFERENCES dev_server_instances(id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    leased_at INTEGER NOT NULL,
    released_at INTEGER
) STRICT;

CREATE TABLE dev_server_events (
    id INTEGER PRIMARY KEY,
    instance_id BLOB NOT NULL,
    config_id BLOB NOT NULL,
    project_id BLOB NOT NULL,
    kind TEXT NOT NULL,
    actor TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;

CREATE UNIQUE INDEX idx_dev_server_active_port
    ON dev_server_port_leases(port) WHERE status='active';
CREATE UNIQUE INDEX idx_dev_server_active_owner
    ON dev_server_port_leases(project_id,ifnull(hex(worktree_id),''),ifnull(hex(task_id),''),config_id)
    WHERE status='active';
CREATE INDEX idx_dev_server_configs_project ON dev_server_configs(project_id,worktree_id,name);
CREATE UNIQUE INDEX idx_dev_server_config_name_owner
    ON dev_server_configs(project_id,ifnull(hex(worktree_id),''),name);
CREATE INDEX idx_dev_server_instances_project ON dev_server_instances(project_id,status,created_at DESC);
CREATE INDEX idx_dev_server_instances_config ON dev_server_instances(config_id,created_at DESC);
CREATE INDEX idx_dev_server_events_instance ON dev_server_events(instance_id,created_at,id);
