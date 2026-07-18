# Retcon development plan

Last reviewed: 2026-07-18

This is the execution plan for the codebase as it exists now. The original phase-by-phase
product roadmap remains in `docs/development-plan.md`; this document replaces it as the
day-to-day development guide. Product scope and non-goals remain governed by
`docs/product/`.

The review included tracked code and the current uncommitted work. Items that are still
uncommitted must be revalidated after they land. Progress is gate-based rather than tied to
calendar estimates.

## 1. Current baseline

Retcon is a pre-alpha, local-first Windows application with three isolated runtime layers:

- Flutter desktop UI in `apps/desktop/` and reusable Flutter packages in `packages/`.
- Rust core, storage, safety, Git, terminal, provider, browser, and verification services in
  `crates/`, plus the Rust CLI in `apps/cli/`.
- Bun/TypeScript/Playwright managed-browser service in `apps/browser-service/`.

The architecture is sound for the MVP: the UI, durable core, and untrusted browser runtime
have separate failure boundaries; local RPC is schema-driven; state is persisted in SQLite;
artifacts are content-addressed; and dangerous operations pass through a permission layer.

Implemented product foundations cover phases 0–27, including project and session
management, agent execution, terminal support, file/Git workflows, checkpoints, approvals,
task planning, test/build verification, development servers, managed Chromium, browser
verification, diagnostics, and external IDE integration. Some earlier completion gates are
still partial even though their core implementation exists.

### Validation snapshot

The following checks were run against the reviewed working tree:

| Surface | Result |
|---|---|
| Rust workspace | 243 tests passed; 2 environment-dependent tests ignored |
| Flutter desktop | Analysis passed; 103 tests passed |
| Browser service | Lint and typecheck passed; 19 tests passed |
| Protocol | Schema valid; 178 methods and 6 fixtures |
| Design system | Analysis passed; 19 tests passed |
| File viewer | Analysis passed; 5 tests passed |
| Diff viewer | Analysis passed; 2 tests passed |
| Terminal view | Analysis passed; 4 tests passed |
| Test dashboard | Analysis passed; 5 tests passed |
| Website | Production build and verification passed |

Two manual/environment gates remain intentionally excluded from the default Rust run: an
authenticated Claude Code flow and an interactive Windows ConPTY flow. They must remain
visible release requirements rather than being treated as ordinary skipped tests.

## 2. Review findings

### Strengths to preserve

- Safety boundaries are explicit. Path canonicalization, scoped approvals, secret redaction,
  checkpoint conflict checks, browser isolation, and bounded logs/artifacts have meaningful
  automated coverage.
- Persistence and recovery are first-class. Sessions, tasks, jobs, approvals, verification,
  browser state, and diagnostics have durable representations and restart tests.
- The protocol is centralized in `schemas/protocol/v1.json`, with generated Dart and
  TypeScript clients checked for drift in CI.
- The verification model does not trust agent assertions; task completion is tied to durable
  acceptance criteria and test/browser evidence.
- The current Windows build job provides an early cross-platform compile signal even though
  packaging is not implemented yet.

### Immediate gaps

1. **The repository-wide CI baseline is implemented locally but has not landed.** The current
   work adds all six Flutter surfaces, browser source/test/script linting, a canonical root
   verification inventory, pinned tool versions, and package-license checks. All affected
   local checks pass; a clean CI checkout must still revalidate the uncommitted changes.
2. **Several product gates remain partial.** Native Win10/11, mixed-DPI, sleep/resume,
   keyboard/screen-reader, detached-window, and interactive terminal evidence is incomplete.
   Safe-change UX still needs a final audit for large file trees/diffs, attribution, rollback,
   notifications, credential storage, and commit/push protections.
3. **Release infrastructure is still skeletal.** `crates/retcon-updater` is a placeholder and
   `.github/workflows/release.yml` emits raw binaries rather than an installer. Code signing,
   update verification, rollback, repair, and clean-machine installation are not present.
4. **Settings and notifications are components, not systems.** Storage has generic settings
   and the design system has notification widgets, but there is no complete validated settings
   hierarchy, Windows notification delivery, focus policy, or user-facing settings workspace.
5. **Complexity is concentrated in large modules.** Examples include
   `crates/retcon-storage/src/repositories.rs`, browser verification across all three runtimes,
   `apps/desktop/lib/src/workspace.dart`, and `apps/desktop/lib/src/desktop_shell.dart`.
   Continued feature work in these files will increase review and regression risk.
6. **Placeholders need explicit ownership.** `crates/retcon-index` and
   `crates/retcon-updater` remain empty foundations. `packages/retcon-plugin-sdk` represents a
   post-MVP phase and must not pull plugin scope into the MVP.

## 3. Delivery priorities

Work in the following order. A milestone is complete only when its exit gate passes and its
evidence is recorded in the repository.

### Milestone A — Establish a trustworthy green baseline (P0)

Goal: make the default checks accurately describe the health of the whole monorepo.

Work:

- Reconcile and land the current working tree in focused changes. Update status documents and
  test counts only after the code is committed and rerun.
- Fix terminal-view deprecated API usage, the test-dashboard analyzer findings, and browser
  test lint findings.
- Add analysis and tests for every shipped Flutter package to CI. Add the test dashboard only
  if it will remain a maintained developer tool.
- Lint both `apps/browser-service/src` and `apps/browser-service/test`.
- Keep protocol generation drift, formatting, Clippy, Rust tests, website verification, and
  the Windows release build mandatory.
- Pin tool versions used by CI where reproducibility matters; avoid a moving `latest` runtime
  in release-producing jobs.
- Add one root developer command that runs the same checks as CI and produces a concise
  summary. The test dashboard may call this inventory but must not define a separate truth.
- Remove the placeholder `TODO` license from `packages/retcon-design-system/LICENSE` and
  verify license/provenance checks cover all distributable packages.

Implementation update (2026-07-18):

- Terminal-view and test-dashboard analyzer findings are fixed, and browser lint covers
  `src`, `test`, and `scripts` without findings.
- The canonical root verification command inventories protocol, Rust, all six Flutter
  surfaces, browser service, website, Windows release builds, and the three explicit manual
  gates. CI consumes the same suite inventory, and the test dashboard is checked for drift.
- Tool versions are pinned for CI, package-license validation is automated, and distributable
  Flutter packages carry the repository Apache-2.0 license.
- Local validation passes: 243 Rust tests (2 environment-dependent tests ignored), 138 Flutter
  tests across six surfaces, 19 browser-service tests, protocol schema/generation checks, and
  the website production verification. Remote CI and clean-checkout evidence remain required
  before closing the milestone.

Exit gate:

- A clean checkout passes every required local and CI check.
- No shipped package has analyzer, compiler, lint, or formatting errors.
- CI and the test dashboard list the same suites and manual gates.
- Documentation reports current, reproducible commands and results.

### Milestone B — Close the end-to-end safety workflow (P0)

Goal: prove the defining user journey before adding release features.

Work:

- Complete and record the Phase 2 hardware/operator matrix: Win10 and Win11, mixed DPI,
  multi-monitor detach/reattach, sleep/resume, native window behavior, keyboard-only use,
  screen reader, and interactive ConPTY ANSI/Unicode/resize/reflow.
- Turn the authenticated provider scenario into a repeatable, opt-in test with clear setup,
  timeout, cleanup, redaction, and recorded supported-version metadata.
- Exercise the complete workflow on a disposable fixture repository: open project, create an
  isolated task/worktree, run an agent, approve an action, inspect files and diffs, checkpoint,
  restore, verify, use managed Chromium, commit, restart, and resume.
- Audit the current safe-change implementation and finish only gaps that remain after current
  work lands:
  - lazy file-tree behavior and large-repository responsiveness;
  - revision-safe inline save and external-change conflict UX;
  - fetch/pull/rebase/push approval and secret-scan paths;
  - multi-scope and paged large diffs with agent-turn attribution;
  - checkpoint preview, scoped restore, conflict explanation, and retention/compaction;
  - approval timeout, batch handling, rule persistence/revocation, and audit display;
  - OS credential-vault integration for provider secrets.
- Add crash/failure variants to the workflow: provider exit, core restart, UI restart, browser
  crash, failed Git operation, dev-server crash, and rejected/timed-out approval.

Exit gate:

- The full workflow passes on clean supported Windows machines without a developer manually
  repairing state.
- Every privileged action is attributable, reviewable, and denied by default when its scope is
  ambiguous.
- Rollback preserves unrelated user edits and reports conflicts before changing files.
- No prompt, source, terminal output, browser content, path, or secret appears in telemetry or
  support bundles.

### Milestone C — Notifications and settings (P1)

Goal: make important state hard to miss and behavior predictably configurable.

Work:

- Write an ADR for settings precedence, sensitive-value storage, notification delivery, and
  Windows integration boundaries.
- Define typed settings contracts instead of exposing arbitrary JSON to features. Support
  global defaults and explicit project overrides with validation and migration.
- Build a settings workspace for startup/restore, appearance/accessibility, terminal/editor,
  provider, approvals, network, Git, verification, browser, retention, diagnostics, and update
  channel settings.
- Store credentials in Windows Credential Manager or an equivalent OS-protected vault; keep
  only opaque references in SQLite.
- Implement a durable notification model with deduplication, severity, read/dismiss state,
  action targets, quiet/focus modes, and per-event preferences.
- Deliver notifications in-app first, then through the Windows notification surface. Approval,
  failure, completion, server crash, and update events must deep-link to the relevant action.
- Ensure notifications never include repository content, prompts, commands, secrets, or raw
  sensitive paths.

Exit gate:

- Settings survive restart, reject invalid values clearly, migrate safely, and obey documented
  global/project precedence.
- Important background events are visible without producing duplicate/noisy alerts.
- Sensitive values never enter ordinary settings, logs, events, exports, or UI snapshots.
- Notification and settings paths pass keyboard, screen-reader, high-contrast, and scaling
  checks.

### Milestone D — Signed updates and Windows distribution (P0 release gate)

Goal: produce a recoverable, verifiable Windows installation rather than loose binaries.

Work:

- Write ADRs for installer technology, update metadata/signing format, channels, rollback, and
  component compatibility policy.
- Implement `retcon-updater` with a narrow state machine: check, download to a private staging
  area, verify metadata/signature/checksum, stage, request restart, apply, health-check, and
  roll back on failure.
- Treat update metadata, packages, mirrors, and downgrade attempts as hostile. Test expired,
  malformed, replayed, truncated, wrong-channel, wrong-architecture, and bad-signature inputs.
- Enforce desktop/core/browser/protocol/database compatibility before mutation and provide a
  repair path for partial installations.
- Build a Windows installer that bundles the desktop, core/CLI, browser service and managed
  runtime, schemas/migrations, assets, notices, and uninstall/repair metadata.
- Support user-level install first. Define data-preservation behavior for upgrade, repair,
  uninstall, downgrade rejection, and failed migration.
- Sign binaries and installer, timestamp signatures, verify them in the release workflow, and
  document certificate/key rotation and emergency revocation.
- Replace the raw-binary draft release with immutable, checksummed, signed release artifacts
  and a promotion flow from development to beta to stable.

Exit gate:

- Install, upgrade, rollback, repair, and uninstall pass on clean Win10/11 VMs as standard user
  and administrator.
- Failed or interrupted updates preserve user data and return to a launchable version.
- CI verifies artifact contents, signatures, checksums, versions, licenses, and provenance.
- No unsigned or incompatible component can be silently installed or launched.

### Milestone E — Security, performance, and release QA (P0 release gate)

Goal: demonstrate that the MVP is safe and usable on real repositories for extended periods.

Work:

- Produce a current threat model covering malicious repositories, agent/provider output, RPC
  spoofing, command/argument injection, path/symlink races, browser compromise, secret theft,
  artifact tampering, update compromise, local privilege boundaries, and denial of service.
- Add adversarial tests at each trust boundary, especially cross-project identity spoofing,
  approval replay, TOCTOU file changes, archive/path traversal, process-tree cleanup, update
  verification, and diagnostic redaction.
- Run dependency/license advisories in CI and establish a documented remediation policy.
- Define measurable budgets before optimizing: startup/reconnect, long conversation rendering,
  terminal throughput, large tree/diff navigation, event replay, memory growth, artifact
  retention, verification runtime, and shutdown cleanup.
- Break up high-churn oversized modules by domain after characterization tests exist. Prioritize
  storage repositories, desktop workspace/shell, and browser-verification orchestration. Do not
  combine these refactors with behavior changes.
- Decide whether `retcon-index` is required for measured large-repository targets. Implement it
  only if simpler lazy filesystem/Git queries miss the budget; otherwise remove/defer the crate.
- Run fault-injection, long-duration, low-disk, offline, restricted-network, antivirus, long-path,
  high-DPI, and multi-monitor scenarios.
- Complete human accessibility and onboarding reviews, then resolve all critical/high findings.

Exit gate:

- No known critical security or data-loss issue remains; high findings have an explicit release
  decision and owner.
- Performance budgets pass on documented reference hardware and large-repository fixtures.
- Twenty-four-hour supervised runs show bounded memory, logs, artifacts, jobs, and child
  processes.
- A new user can install, configure the provider, complete the defining workflow, recover from
  common failures, and export safe diagnostics without internal documentation.

### Milestone F — Private alpha (P1)

Goal: validate the product with a small, consented group before public distribution.

Work:

- Publish installation, supported-provider, known-limitations, privacy, security-reporting,
  recovery, diagnostics, and uninstall documentation.
- Create a release checklist, rollback procedure, incident process, support intake template,
  and reproducible bug-report bundle.
- Recruit users with varied repository sizes and Windows/display configurations.
- Collect only explicit, opt-in, enumerated telemetry. Prefer user-submitted support bundles and
  structured interviews during alpha.
- Track task completion, approval abandonment, recovery success, install/update failures,
  provider/browser failures, performance regressions, and data-loss/security incidents.
- Fix alpha blockers before expanding provider count or platform scope.

Exit gate:

- Alpha users complete real tasks end to end and can recover from common failures.
- Installation/update failure, crash, recovery, and security signals meet written thresholds.
- The issue backlog has clear severity, ownership, reproduction evidence, and release decisions.

## 4. Required quality matrix

The root verification command and CI should converge on this matrix:

| Frequency | Required checks |
|---|---|
| Every change | Protocol validation/generation drift; Rust fmt, Clippy, and workspace tests; analysis and tests for every Flutter app/package; browser lint/typecheck/tests; website build verification |
| Windows CI | Release builds; core/UI startup and handshake; installer smoke test once available; process cleanup; selected filesystem/Git/ConPTY tests |
| Scheduled | Managed Chromium E2E; dependency/license audit; large-repository and long-session benchmarks; retention/cleanup; fault injection |
| Release candidate | Authenticated provider E2E; clean-machine install/upgrade/rollback/uninstall; full Win10/11 and display matrix; accessibility review; security checklist; defining workflow |

Tests should assert user-visible outcomes and durable state, not private implementation details.
Every fixed security, recovery, or data-loss defect requires a regression test at the lowest
useful boundary plus an end-to-end test when the cross-process contract was involved.

## 5. Engineering rules for new work

- Keep MVP scope frozen. Notifications, settings, updater, packaging, security, and QA are the
  remaining product phases; multi-provider, multi-agent, cloud, GitHub integration, recipes,
  memory, and plugins remain post-MVP.
- Preserve the three-process trust boundary. UI convenience must not bypass core validation,
  permissions, audit, persistence, or browser isolation.
- Change `schemas/protocol/v1.json` first for RPC contract changes; regenerate clients and add
  valid and invalid fixtures in the same change.
- Put identity and authorization scope in durable server-owned state, never in caller claims.
- Bound all collections, messages, logs, artifacts, timeouts, concurrency, and external process
  output. Define cleanup on success, failure, timeout, cancellation, and restart.
- Migrate persisted data forward transactionally. Back up before destructive migrations and
  test older/current/newer/corrupt database cases.
- Refactor large modules behind characterization tests and small stable interfaces. Prefer
  domain modules over new cross-cutting managers.
- Record significant, hard-to-reverse Windows, security, storage, protocol, installer, and
  update decisions as ADRs.
- Keep documentation evidence-based: “implemented” means the relevant automated and manual
  completion gates have passed, not merely that supporting code exists.

## 6. Risk register

| Risk | Current signal | Mitigation / owner area |
|---|---|---|
| Windows-only behavior is under-tested | Manual platform matrix remains partial | Windows CI, VM matrix, recorded human evidence |
| Provider CLI/version drift | One live provider path; authenticated test is opt-in | Capability/version policy, scheduled supported-version E2E |
| UI/core/browser contract drift | 178 RPC methods across three runtimes | Schema-first changes, generated clients, invalid fixtures, E2E |
| Oversized orchestration modules | Multiple 1,000–2,000-line files | Characterization tests and domain extraction before feature growth |
| Unsafe update/distribution chain | Updater placeholder; raw binaries only | Signed metadata/artifacts, rollback, installer and adversarial tests |
| Secret leakage through evidence/diagnostics | Many content-bearing surfaces | Central sanitization, canary tests, export preview, vault references |
| Long-session resource growth | Bounded components exist; endurance evidence incomplete | 24-hour tests, budgets, retention and orphan-process assertions |
| Scope creep before alpha | Post-MVP placeholders already exist | Enforce `docs/product/scope.md`; defer plugin/index work unless gated |

## 7. Definition of MVP readiness

Retcon is ready for private alpha only when a clean supported Windows machine can install a
signed build and a user can open a repository, configure the supported provider, delegate a
task in an isolated worktree, observe and stop it, approve or deny risky actions, review and
restore changes, run test/build/browser verification, commit approved work, restart and resume,
recover from provider/core/browser failures, and export redacted diagnostics. The application
must then update or roll back safely without losing project or session data.

No release label should override an unmet safety, data-loss, signing, compatibility, or defining
workflow gate.
