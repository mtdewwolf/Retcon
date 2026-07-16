# Policies

These policies govern what data Retcon collects, keeps, and protects. They are commitments,
not defaults to be quietly changed. They tie directly to the security principles in
[principles.md](principles.md) and to the diagnostics and privacy work in later phases.

## Telemetry policy

**Opt-in, off by default.**

- Retcon collects **no telemetry** unless the user explicitly enables it.
- Nothing leaves the machine as a result of using the product until the user turns
  telemetry on.
- When enabled, the collected fields are documented in-product and in these docs, and the
  user can disable telemetry and delete previously collected local telemetry at any time.
- Redaction is mandatory before any transmission: **file contents, secrets, and — where
  possible — usernames and machine identifiers are redacted.** Telemetry never includes
  source code or repository contents.

## Crash-reporting policy

**Opt-in, redacted.**

- Crash reports are **not** sent automatically. Automatic crash upload is off by default and
  requires explicit consent, separate from product telemetry.
- Crash bundles are redacted using the same rules as telemetry (no secrets, no file
  contents).
- A **local support-bundle export** is always available without any upload, so users can
  diagnose or share problems on their own terms. Support bundles exclude secrets by
  construction.

## Data-retention policy

Retcon stores its data **locally**:

- **SQLite database** for structured state (projects, sessions, turns, tasks, approvals,
  permission rules, terminal/command history, worktrees, checkpoints, browser sessions,
  diagnostics, settings, and project memories).
- **Content-addressed artifact storage** for logs, screenshots, diagnostic bundles, file
  snapshots, browser traces, patches, and test reports.

Retention commitments:

- Retention is **user-controllable**, with a cleanup job and disk-usage reporting.
- Cleanup **never removes active artifacts** (those referenced by a running job or a live
  session).
- The user can delete logs, artifacts, and project memory.
- Sensitive values are not kept in plaintext local storage — they live in the **OS
  credential vault**; other sensitive local values are encrypted, and temporary secrets are
  deleted after use.

## Secrets and privacy stance

- **Scan before transmit.** Context is scanned and secrets redacted before any transmission
  to a provider; the user can see which files are included in provider context and the
  destination.
- **Scan before commit and push.** Diffs are scanned for secrets before commit and before
  push; confirmed secrets are blocked, with a recorded override path for false positives.
- **No secrets in logs.** Logs, diagnostic bundles, and artifacts are redacted; artifact
  and temporary-file permissions are restricted; clipboard leakage is avoided where
  possible.
