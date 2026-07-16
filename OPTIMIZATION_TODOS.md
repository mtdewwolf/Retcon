# Optimization TODO list

Full codebase review after UUID-as-BLOB migration (`e2f6677`).
Priorities: **P0** = correctness / unbounded growth; **P1** = hot-path performance;
**P2** = medium impact; **P3** = polish / future.

Stub crates (`retcon-worktrees`, `retcon-browser`, `retcon-checkpoints`,
`retcon-filesystem`, `retcon-index`, `retcon-platform`, `retcon-permissions`,
`retcon-diagnostics`, `retcon-updater`) and empty TypeScript packages have no
meaningful optimization surface yet.

---

## Done in this PR

### Growth / correctness

- [x] **P0** Bound durable event persist queue (`sync_channel` + `try_send`)
  (`crates/retcon-core/src/event.rs`)
- [x] **P0** Bound terminal PTY output queue with drop counters
  (`crates/retcon-core/src/spikes/terminal.rs`)
- [x] **P0** Truncate browser console/network strings before buffering
  (`apps/browser-service/src/browser.ts`)
- [x] **P0** Cap browser-service stdin RPC line size (`main.ts`)

### Hot path

- [x] **P1** Move event retention prune onto the writer thread (`event.rs`)
- [x] **P1** Batch durable event inserts in a writer transaction (`event.rs`)
- [x] **P1** Drop event-bus mutex before cloning replay payloads (`event.rs`)
- [x] **P1** Stream spike events via `emit_volatile` (terminal/agent/browser)
- [x] **P1** Job SQLite persistence via `block_in_place` on Tokio (`jobs.rs`)
- [x] **P1** Shutdown DB maintain + artifact cleanup off async workers
- [x] **P1** Real stdio concurrency: status/logs during mutating browser ops
- [x] **P1** UUID BLOB reads as `[u8; 16]` (no per-column `Vec` alloc)
- [x] **P1** Browser launch failure cleans up temp profile; clear logs on close

### Medium / polish shipped here

- [x] **P2** Cap Git stderr the same way as stdout (`retcon-git`)
- [x] **P2** Cap agent provider line length (`retcon-agents`)
- [x] **P2** Artifact reference scan uses `UNION ALL` + Rust `HashSet`
- [x] **P2** `mem::take` for coalesced terminal buffers
- [x] **P2** Fix post-v4 schema assertions + BLOB UUID test inserts
- [x] **P3** Desktop `CoreClient` tears down socket/heartbeat on failure
- [x] **P3** Fix `writeLine` double-resolve on accepted stdout writes

---

## Remaining TODOs

### P1 — Hot path

- [ ] **P1** Dedicated job-persistence writer actor (replace `block_in_place`)
  - Files: `crates/retcon-core/src/jobs.rs`
  - Why: matches the event-bus pattern and enables batched job upserts.

### P2 — Medium

- [ ] **P2** Order-preserving / SQL-backed `jobs.list` pagination
  - Files: `crates/retcon-core/src/jobs.rs`, `server.rs`
  - Why: still clones + sorts the entire in-memory map before `skip/take`.

- [ ] **P2** Split event prune SQL (age delete vs count trim) + `created_at` index
  - Files: `crates/retcon-core/src/event.rs`, new migration
  - Why: `OR id NOT IN (… ORDER BY id DESC LIMIT …)` is planner-hostile.

- [ ] **P2** BLOB-native bootstrap schema for fresh DBs (skip TEXT→BLOB rebuild)
  - Files: `0001_initial.sql` / `database.rs` `migrate_uuid_columns`
  - Why: every new DB still creates TEXT UUIDs then rebuilds all tables.

- [ ] **P2** Single-pass RPC frame serialize (avoid `Value` intermediate)
  - Files: `crates/retcon-core/src/server.rs`
  - Why: every response/event pays double JSON encode cost.

- [ ] **P2** Reduce `emit_internal` double-`Arc` / envelope clone
  - Files: `crates/retcon-core/src/event.rs`
  - Why: assigns sequence then rebuilds `Arc` via full envelope clone.

- [ ] **P2** Fixed-index ring buffer instead of `splice(0, n)` in browser logs
  - Files: `apps/browser-service/src/browser.ts` (`pushCapped`)
  - Why: O(n) copy on every overflow under chatty pages.

- [ ] **P2** Attach console/network listeners on every context page (popups)
  - Files: `apps/browser-service/src/browser.ts`
  - Why: only the first page is instrumented today.

- [ ] **P2** CI: cache Playwright browsers + path-filter jobs
  - Files: `.github/workflows/ci.yml`
  - Why: cold Chromium install and full-matrix runs dominate PR time.

### P3 — Polish

- [ ] **P3** Cap concurrent core RPC connections
  - Files: `crates/retcon-core/src/server.rs`

- [ ] **P3** Paginate `sessions.list_for_project`
  - Files: `crates/retcon-storage/src/repositories.rs`

- [ ] **P3** Read durable `category` column on event hydrate
  - Files: `crates/retcon-core/src/event.rs` (`load_recent_events`)

- [ ] **P3** Make `RetconApp` own `CoreClient`/`GoRouter` in `initState`
  - Files: `apps/desktop/lib/main.dart`

- [ ] **P3** Idempotent desktop `initLogging()`
  - Files: `apps/desktop/lib/src/logging.dart`

- [ ] **P3** Localize desktop start-menu `setState` / cache theme
  - Files: `desktop_shell.dart`, `theme.dart`

- [ ] **P3** Website/asset CI short-circuits (mtime/hash) + pin Bun
  - Files: `apps/website/scripts/`, `.github/workflows/`

- [ ] **P3** Prefer ephemeral `chromium.launch` when no profile is required
  - Files: `apps/browser-service/src/browser.ts`

---

## Suggested order of attack

1. Job writer actor (finish matching event-bus durability pattern)
2. BLOB-native fresh schema + prune SQL split (storage cold-start / retention)
3. RPC encode + emit Arc cleanup (steady-state CPU)
4. Browser ring buffer + page listeners (evidence completeness)
5. CI Playwright cache / path filters (developer loop)
