# Retcon

**Retcon** is an open-source desktop application for **running, supervising, and verifying
AI coding agents** — a control room for delegating real coding tasks to an agent, inspecting
everything it does, verifying the result in a managed browser, and recovering cleanly when
something breaks.

> Status: **pre-alpha** — Phases 0–5 and 6–14 (foundation) implemented with automated
> test evidence; Phases 15–20 (safe code changes) landed in Wave 4 with gate-partial
> coverage (Milestone 4 foundation). Phases 21–25 task planning, verification,
> dev-server management, managed browser service, and browser verification are implemented
> and verified; Phase 26 diagnostics and observability is implemented and verified; Phase 27 IDE
> integration and file synchronization is underway. Phase 2 platform matrix and Milestone 3
> live E2E evidence remain pending. See `docs/phase-4-20-tracker.md`.

## What Retcon will do

- Run coding agents in **isolated Git worktrees**, with every command and file change
  attributed and reviewable.
- Pause on risky actions with **explicit permission prompts**; nothing dangerous happens
  without approval.
- Provide **checkpoints and rollback** for any agent turn.
- Verify results with **evidence**: tests, builds, and screenshots from a managed Chromium
  browser — not the agent's say-so.
- **Survive crashes** without losing sessions, and stay honest about failures.

The full product definition lives in the maintainers' internal `docs/` directory.

## Architecture

| Component | Location | Stack |
|-----------|----------|-------|
| Desktop shell | `apps/desktop/` | Flutter / Dart (XP-inspired dark theme; not Electron) |
| Core service | `crates/retcon-core/` | Rust + Tokio |
| CLI | `apps/cli/` | Rust |
| Browser service | `apps/browser-service/` | TypeScript on Bun + Playwright (managed Chromium) |
| Marketing website | `apps/website/` | Astro static site (GitHub Pages) |
| Shared UI packages | `packages/` | Dart/Flutter |
| Protocol & schemas | `schemas/` | Typed local RPC over named pipes / Unix sockets |

Local data lives in SQLite plus content-addressed artifact storage. Windows 10/11 first;
Linux and macOS later.

## Repository layout

```text
retcon/
├── apps/            # desktop (Flutter), cli (Rust), browser-service (Bun/TS)
├── crates/          # Rust workspace crates (retcon-core, retcon-protocol, …)
├── packages/        # Dart/Flutter packages (design system, viewers, …)
├── schemas/         # protocol / workflow / plugin schemas
├── docs/            # product docs, ADRs, architecture
├── examples/        # example projects and fixtures for manual testing
├── scripts/         # development and CI scripts
└── tests/           # cross-component test fixtures
```

## Building

Prerequisites: Rust (pinned via `rust-toolchain.toml`), Flutter (stable, Windows desktop
enabled), Bun ≥ 1.1, Git.

```sh
# Rust core + CLI
cargo build --workspace

# Desktop shell
cd apps/desktop && flutter run -d windows

# Browser service
cd apps/browser-service && bun install && bun run start

# Marketing website
cd apps/website && npm install && npm run dev
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Security reports: [SECURITY.md](SECURITY.md).

## License

Apache-2.0 — see [LICENSE](LICENSE).
