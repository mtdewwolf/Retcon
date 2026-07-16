# 0001 — Core technology stack

- **Status:** Accepted
- **Date:** 2026-07-15

## Context

Retcon needs a native-feeling Windows desktop shell, a durable long-running local service
that supervises agent processes, PTYs, Git, and storage, and isolated browser automation —
without becoming an Electron app, depending on WebView2 for the primary UI, or building a
rendering engine (see [product principles](../product/principles.md)).

## Decision

- **Desktop shell:** Flutter/Dart, frameless custom window, XP-inspired dark theme.
- **Core engine:** Rust with the Tokio async runtime, run as a separate process from the UI.
- **Local communication:** typed local RPC over Windows named pipes (Unix domain sockets on
  Linux/macOS later), with generated types shared across Rust, Dart, and TypeScript.
- **Local data:** SQLite for structured state; content-addressed artifact storage for logs,
  screenshots, snapshots, traces.
- **Browser automation:** managed Chromium driven by Playwright, in a separate TypeScript
  service running on Bun.
- **Git:** native Git CLI as the source of truth; optional `git2` for lightweight reads.
- **Initial platform:** Windows 10/11 only.

## Alternatives considered

- **Electron / Tauri (WebView) shell** — rejected by product principle; Electron is a
  non-goal, and WebView2 dependence for the primary UI is disallowed.
- **All-in-one Rust UI (egui/Slint)** — weaker ecosystem for the rich, themed desktop shell
  Retcon needs; Flutter has mature desktop support and a strong widget model.
- **UI and core in one process** — rejected: a crash in either must not take down the
  other, and the core must outlive UI restarts (crash-recovery principle).
- **CDP directly instead of Playwright** — more control, far more maintenance; Playwright's
  managed Chromium and auto-waiting are worth the dependency for the MVP.

## Consequences

- Three runtimes (Dart, Rust, TS) mean protocol types must be generated, not hand-written
  (Phase 4), and CI must cover all three.
- Flutter terminal rendering performance is a known top risk (see
  [risks.md](../product/risks.md)); PTY logic stays in Rust and the renderer must remain
  replaceable.
- Process separation gives crash isolation for free but makes lifecycle management
  (single-instance, orphan cleanup) a first-class concern (Phase 3).
