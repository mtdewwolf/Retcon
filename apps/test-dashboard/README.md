# Retcon Test Dashboard

Visual dashboard for Retcon's multi-stack test suite. Discover test files, run suites, and see what needs attention — failures, suites not in CI, and manual-only matrix items.

## Run

From the repository root:

```powershell
cd apps/test-dashboard
flutter pub get
flutter run -d windows
```

Or set `RETCON_ROOT` if launching from outside the repo:

```powershell
$env:RETCON_ROOT = "C:\path\to\Retcon"
flutter run -d windows
```

## Features

- **Suite inventory** — Rust, Flutter, Bun, Node, and protocol suites aligned with CI and phase-2 scripts
- **File discovery** — scans test files per suite
- **Run controls** — run one suite, all suites, or rerun failed
- **Output parsing** — normalizes cargo, flutter, and bun output into pass/fail lists
- **Focus panel** — prioritized list of failures, never-run suites, not-in-CI gaps, and manual matrix items

## Layout

| Pane | Purpose |
|------|---------|
| Left | Suite list with stack filters and status badges |
| Center | Selected suite detail, test case tabs, discovered files |
| Right | Focus items ranked by priority |
