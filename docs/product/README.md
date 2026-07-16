# Retcon — Product Documentation

Retcon is an open-source desktop application for **running, supervising, and verifying AI
coding agents**. It combines a Windows XP-inspired dark desktop shell (Flutter) with a
durable Rust core service and a managed Chromium browser service, so a developer can
delegate a real coding task to an AI agent, inspect everything the agent does, verify the
resulting application, and recover cleanly when something breaks.

This folder holds the **Phase 0 product definition** — the canonical statement of what
Retcon is, what the MVP includes and excludes, and the principles and policies that govern
it. Read it before any engineering work in Phase 1.

## Reading order

| Doc | What it covers |
|-----|----------------|
| [vision.md](vision.md) | Product vision, target user, primary use cases, competitive position |
| [mvp-scope.md](mvp-scope.md) | MVP inclusions, exclusions, non-goals, and Definition of Done |
| [principles.md](principles.md) | Product, security, and user-experience principles |
| [platforms-and-agents.md](platforms-and-agents.md) | Supported operating systems and coding agents; provider strategy |
| [business-model.md](business-model.md) | Open-source model, license recommendation, pricing |
| [policies.md](policies.md) | Telemetry, crash-reporting, data-retention, and secrets/privacy policies |
| [risks.md](risks.md) | Highest-risk technical areas and their mitigations |
| [milestones.md](milestones.md) | The eight delivery milestones and recommended build order |

## Technology at a glance

- **Desktop interface:** Flutter / Dart (frameless custom-shell window; **not** Electron, **not** WebView2 for the primary UI, **not** a custom renderer).
- **Core engine:** Rust with the Tokio async runtime.
- **Local communication:** typed local RPC over Windows named pipes (Unix domain sockets on Linux/macOS later).
- **Local data:** SQLite plus content-addressed artifact storage.
- **Browser automation:** managed Chromium driven by Playwright (TypeScript on Bun), isolated from the desktop shell.
- **Git:** native Git CLI, with optional `git2` for lightweight operations.
- **Initial platform:** Windows 10 and Windows 11.

## Locked-in Phase 0 decisions

- **Source model:** fully open source (see [business-model.md](business-model.md)).
- **Telemetry:** opt-in, off by default (see [policies.md](policies.md)).
- **Pricing:** free; all AI cost is the user's own provider account — bring-your-own-provider.

## Phase 0 completion gate

| Gate item | Satisfied by | Status |
|-----------|-------------|--------|
| Product vision approved | [vision.md](vision.md) | ✅ Documented — pending user sign-off |
| MVP scope frozen | [mvp-scope.md](mvp-scope.md) | ✅ Documented — pending user sign-off |
| Initial architecture documented | Technology section above + supplied roadmap | ✅ Documented |
| Technical risks documented | [risks.md](risks.md) | ✅ Documented |
| Initial milestones agreed | [milestones.md](milestones.md) | ✅ Documented |
| Repository created | Phase 1 (repo + monorepo scaffolding) | ⏳ Deferred to Phase 1 — **not yet met** |

> Phase 0 is a **documentation-only** deliverable. Creating the repository, monorepo
> structure, build tooling, CI, and the `LICENSE` file are Phase 1 work.
