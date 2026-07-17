-- Phase 21: durable task planning, dependency tracking, and acceptance gates.

ALTER TABLE tasks ADD COLUMN project_id BLOB REFERENCES projects(id) ON DELETE CASCADE;
ALTER TABLE tasks ADD COLUMN parent_task_id BLOB REFERENCES tasks(id) ON DELETE SET NULL;
ALTER TABLE tasks ADD COLUMN worktree_id BLOB REFERENCES git_worktrees(id) ON DELETE SET NULL;
ALTER TABLE tasks ADD COLUMN branch TEXT;
ALTER TABLE tasks ADD COLUMN worktree_path TEXT;
ALTER TABLE tasks ADD COLUMN agent TEXT;
ALTER TABLE tasks ADD COLUMN provider TEXT;
ALTER TABLE tasks ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN estimated_cost_micros INTEGER CHECK(estimated_cost_micros >= 0);
ALTER TABLE tasks ADD COLUMN actual_cost_micros INTEGER CHECK(actual_cost_micros >= 0);
ALTER TABLE tasks ADD COLUMN cost_currency TEXT NOT NULL DEFAULT 'USD';
ALTER TABLE tasks ADD COLUMN started_at INTEGER;

-- Existing tasks inherit the project of their attached session. Tasks without a
-- session remain valid global tasks and may be assigned to a project later.
UPDATE tasks
SET project_id = (
    SELECT sessions.project_id FROM sessions WHERE sessions.id = tasks.session_id
)
WHERE session_id IS NOT NULL;

ALTER TABLE task_steps ADD COLUMN description TEXT;

ALTER TABLE acceptance_criteria ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
ALTER TABLE acceptance_criteria ADD COLUMN is_required INTEGER NOT NULL DEFAULT 1 CHECK(is_required IN (0, 1));
ALTER TABLE acceptance_criteria ADD COLUMN evaluated_at INTEGER;
ALTER TABLE acceptance_criteria ADD COLUMN override_reason TEXT;
ALTER TABLE acceptance_criteria ADD COLUMN overridden_by TEXT;
ALTER TABLE acceptance_criteria ADD COLUMN overridden_at INTEGER;

CREATE TABLE task_dependencies (
    task_id BLOB NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    depends_on_task_id BLOB NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(task_id, depends_on_task_id),
    CHECK(task_id <> depends_on_task_id)
) WITHOUT ROWID, STRICT;

CREATE TABLE task_step_dependencies (
    step_id BLOB NOT NULL REFERENCES task_steps(id) ON DELETE CASCADE,
    depends_on_step_id BLOB NOT NULL REFERENCES task_steps(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(step_id, depends_on_step_id),
    CHECK(step_id <> depends_on_step_id)
) WITHOUT ROWID, STRICT;

CREATE INDEX idx_tasks_project_status_updated
    ON tasks(project_id, status, updated_at DESC);
CREATE INDEX idx_task_dependencies_prerequisite
    ON task_dependencies(depends_on_task_id);
CREATE INDEX idx_task_steps_task_sequence
    ON task_steps(task_id, sequence);
CREATE INDEX idx_task_step_dependencies_prerequisite
    ON task_step_dependencies(depends_on_step_id);
CREATE INDEX idx_acceptance_criteria_task_order
    ON acceptance_criteria(task_id, sort_order, updated_at);
