-- Speed up permission engine fingerprint lookups that currently scan
-- request_json via json_extract on every protected RPC.
CREATE INDEX IF NOT EXISTS idx_approvals_pending_fingerprint
  ON approvals(json_extract(request_json, '$.fingerprint'))
  WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_approvals_approved_fingerprint
  ON approvals(json_extract(request_json, '$.fingerprint'))
  WHERE status = 'approved';

CREATE INDEX IF NOT EXISTS idx_permission_rules_project_expires
  ON permission_rules(project_id, expires_at);
