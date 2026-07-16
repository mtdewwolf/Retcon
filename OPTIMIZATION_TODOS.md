# Optimization TODO list

Full codebase review after `1c81051` (app-flow refactor + prior growth fixes).
Priorities: **P0** = correctness / unbounded growth; **P1** = hot-path performance;
**P2** = medium impact; **P3** = polish / future.

Stub crates (`retcon-worktrees`, `retcon-browser`, `retcon-checkpoints`,
`retcon-filesystem`, `retcon-index`, `retcon-platform`, `retcon-permissions`,
`retcon-diagnostics`, `retcon-updater`) and empty TypeScript packages have no
meaningful optimization surface yet.

---

## Done in this PR

- [x] **P0** Bound durable event persist queue (`sync_channel` + `try_send`)
  (`crates/retcon-core/src/event.rs`)
- [x] **P0** Bound terminal PTY output queue with drop counters
  (`crates/retcon-core/src/spikes/terminal.rs`)
- [x] **P1** Move event retention prune onto the writer thread (`event.rs`)
- [x] **P1** Drop event-bus mutex before cloning replay payloads (`event.rs`)
- [x] **P1** Job SQLite persistence via `block_in_place` on Tokio (`jobs.rs`)
- [x] **P1** Shutdown artifact cleanup uses `cleanup_referenced_async`
  (`lifecycle.rs`)
- [x] **P1** Browser launch failure cleans up temp profile (`browser.ts`)
- [x] **P1** Desktop `CoreClient` tears down socket/heartbeat on failure/disconnect
  (`apps/desktop/lib/src/core_client.dart`)
- [x] **P2** Cap Git stderr the same way as stdout (`retcon-git`)
- [x] **P2** Cap agent provider line length (`retcon-agents`)
- [x] **P2** Artifact reference scan uses `UNION ALL` + Rust `HashSet`
  (`artifacts.rs`)
- [x] **P3** Clear browser console/network buffers on close/relaunch (`browser.ts`)

---

## Remaining TODOs

### P1 — Hot path

- [ ] **P1** Dedicated job-persistence writer (or async flush) instead of
  `block_in_place` per transition
  - Files: `crates/retcon-core/src/jobs.rs`
  - Why: `block_in_place` avoids starving the runtime but still couples job
    lifecycle latency to SQLite; a writer actor would match the event-bus pattern
    and allow batching.

- [ ] **P1** Batch durable event inserts on the writer thread
  - Files: `crates/retcon-core/src/event.rs` (`event_writer_loop`)
  - Why: one INSERT + prepare per event; draining a small batch inside a
    transaction would raise SQLite throughput under terminal/agent floods.

### P2 — Medium

- [ ] **P2** Order-preserving / SQL-backed `jobs.list` pagination
  - Files: `crates/retcon-core/src/jobs.rs`, `server.rs`
  - Why: still clones + sorts the entire in-memory map before `skip/take`.

- [ ] **P2** Batch SQLite event prune with inserts (single writer transaction)
  - Files: `crates/retcon-core/src/event.rs`
  - Why: prune already runs on the writer; folding it into periodic batched
    write transactions reduces connection churn further.

### P3 — Polish

- [ ] **P3** Make `RetconApp` own `CoreClient`/`GoRouter` in `initState`
  - Files: `apps/desktop/lib/main.dart`
  - Why: `build()` + `ChangeNotifierProvider.value` recreates/owns incorrectly
    when `core` is null (tests today; production risk later).

- [ ] **P3** Idempotent `initLogging()` (guard / dispose subscription)
  - Files: `apps/desktop/lib/src/logging.dart`
  - Why: repeated init stacks root listeners for process lifetime.

- [ ] **P3** Website build: parallel Sharp metadata + precomputed proof manifest
  - Files: `apps/website/src/pages/index.astro`,
    `apps/website/scripts/check-launch-assets.mjs`
  - Why: build-time only; serial `existsSync` / Sharp probes.

- [ ] **P3** UUID-as-BLOB schema migration to shrink indexes
  - Files: `crates/retcon-storage/src/repositories.rs` (+ coordinated migration)
  - Why: deferred until a dedicated schema sprint; touches every TEXT UUID column.

---

## Suggested order of attack

1. Job writer actor + batched event inserts (throughput under load)
2. `jobs.list` ordered pagination (RPC polling cost)
3. Desktop ownership / logging polish (leak hygiene)
4. Schema UUID-as-BLOB when doing the next storage migration
