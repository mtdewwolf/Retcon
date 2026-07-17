-- Immutable task acceptance history. Deliberately has no cascading foreign key:
-- audit records survive criterion and task deletion.
CREATE TABLE acceptance_criterion_events (
    id INTEGER PRIMARY KEY,
    criterion_id BLOB NOT NULL,
    task_id BLOB NOT NULL,
    kind TEXT NOT NULL,
    actor TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_acceptance_events_criterion_created
    ON acceptance_criterion_events(criterion_id, created_at, id);
CREATE INDEX idx_acceptance_events_task_created
    ON acceptance_criterion_events(task_id, created_at, id);
