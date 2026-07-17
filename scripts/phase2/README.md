# Phase 2 validation scripts

Runnable checks for the six manual matrix items in [`docs/phase-0-3-audit.md`](../../docs/phase-0-3-audit.md).
Invoked from a developer machine on Windows 10/11; CI runs the headless subset via
[`run-automated.ps1`](./run-automated.ps1).

Each script prints `PASS` or `FAIL` on the last line and exits with code 0 or 1.

| Script | Matrix # | Mode |
|--------|----------|------|
| `01-windows-shell.ps1` | 1 | Interactive (human + screen recording) |
| `02-multi-monitor-sleep.ps1` | 2 | Interactive (human; captures layout JSON) |
| `03-terminals.ps1` | 3 | Mixed (automated ConPTY test + interactive shells) |
| `04-agent-e2e.ps1` | 4 | Mixed (`RETCON_AGENT_E2E=1` for full turn) |
| `05-browser-service.ps1` | 5 | Automated (Playwright managed Chromium) |
| `06-git-large-repo.ps1` | 6 | Automated (timing + conflict/submodule fixtures) |
| `run-automated.ps1` | all | Headless CI/developer sweep |

Quick start:

```powershell
cd C:\path\to\Retcon
.\scripts\phase2\run-automated.ps1
.\scripts\phase2\03-terminals.ps1
.\scripts\phase2\05-browser-service.ps1
# Then run 01, 02, and interactive portions of 03–04 on real hardware.
```
