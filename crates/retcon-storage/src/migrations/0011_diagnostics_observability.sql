-- Phase 26: bounded local diagnostics and performance metrics.

ALTER TABLE diagnostics ADD COLUMN event TEXT NOT NULL DEFAULT 'legacy'
    CHECK(length(event) BETWEEN 1 AND 128);
ALTER TABLE diagnostics ADD COLUMN fields_json TEXT NOT NULL DEFAULT '{}'
    CHECK(json_valid(fields_json) AND json_type(fields_json)='object' AND length(fields_json)<=16384);
CREATE TABLE diagnostic_metrics (
    id BLOB PRIMARY KEY CHECK(length(id)=16),
    session_id BLOB REFERENCES sessions(id) ON DELETE SET NULL,
    recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
    component TEXT NOT NULL CHECK(length(component) BETWEEN 1 AND 128),
    name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 128),
    value REAL NOT NULL,
    unit TEXT NOT NULL CHECK(unit IN ('milliseconds','count','bytes','ratio')),
    dimensions_json TEXT NOT NULL DEFAULT '{}'
        CHECK(json_valid(dimensions_json) AND json_type(dimensions_json)='object' AND length(dimensions_json)<=16384),
    CHECK(unit<>'ratio' OR (value>=0.0 AND value<=1.0))
) STRICT;

CREATE INDEX idx_diagnostics_created
    ON diagnostics(created_at DESC,id DESC);
CREATE INDEX idx_diagnostics_source_event_created
    ON diagnostics(source,event,created_at DESC,id DESC);
CREATE INDEX idx_diagnostics_severity_created
    ON diagnostics(severity,created_at DESC,id DESC);
CREATE INDEX idx_diagnostic_metrics_recorded
    ON diagnostic_metrics(recorded_at DESC,id DESC);
CREATE INDEX idx_diagnostic_metrics_component_name_recorded
    ON diagnostic_metrics(component,name,recorded_at DESC,id DESC);
