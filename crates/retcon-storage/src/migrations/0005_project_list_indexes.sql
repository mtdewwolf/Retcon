-- Active project listing ordered by recency.
CREATE INDEX IF NOT EXISTS idx_projects_active_updated
    ON projects(updated_at DESC)
    WHERE archived_at IS NULL;

-- Project metadata hydration for project.list.
CREATE INDEX IF NOT EXISTS idx_settings_key ON settings(key);
