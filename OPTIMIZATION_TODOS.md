# Optimization TODO list

Full codebase review after floating terminal/browser panels (`f542b35`)
and prior SQLite/event-bus work. Priorities: **P0** = correctness /
unbounded growth; **P1** = hot-path performance; **P2** = medium impact;
**P3** = polish / future.

Stub crates (`retcon-worktrees`, `retcon-browser`, `retcon-checkpoints`,
`retcon-filesystem`, `retcon-index`, `retcon-platform`, `retcon-permissions`,
`retcon-diagnostics`, `retcon-updater`) and empty TypeScript/Dart packages
have no meaningful optimization surface yet.

---

## Done in this PR

- [x] **P0** Cap RPC frame reads with chunked `fill_buf` (no unbounded
  `read_until`) — `crates/retcon-core/src/frame.rs`
- [x] **P0** Cap Git stderr; kill child on stdout/stderr overflow —
  `crates/retcon-git/src/lib.rs`
- [x] **P0** Bound event persist queue (`sync_channel` + `try_send`) —
  `crates/retcon-core/src/event.rs`
- [x] **P0** Bound terminal output channel + drop-on-full —
  `crates/retcon-core/src/spikes/terminal.rs`
- [x] **P0** Fix agent cancel deadlock (don't hold mutex across `wait`) —
  `crates/retcon-core/src/spikes/agent.rs`
- [x] **P0** Cap browser-service stdin RPC line size —
  `apps/browser-service/src/main.ts`
- [x] **P0** Cap browser console/network/text field bytes —
  `apps/browser-service/src/browser.ts`
- [x] **P1** Real stdio concurrency for status/logs vs mutating methods —
  `apps/browser-service/src/main.ts`
- [x] **P1** Don't emit `browser.crashed` on intentional close —
  `apps/browser-service/src/browser.ts`
- [x] **P1** Default `browser.logs` to latest tail —
  `apps/browser-service/src/browser.ts`
- [x] **P1** Workspace restore race / dispose guard —
  `apps/desktop/lib/src/workspace.dart`
- [x] **P1** Fix float/close singleton tab replacement bug —
  `apps/desktop/lib/src/workspace.dart`
- [x] **P1** Reuse existing floating terminal/browser panels —
  `apps/desktop/lib/src/workspace.dart`
- [x] **P1** Wire tab switching + cap closed-panel history —
  `apps/desktop/lib/src/workspace.dart`

---

## Still open

### P1

- [ ] **P1** Dedicated job-persistence writer actor (replace sync DB writes
  / any `block_in_place` on async paths) — `crates/retcon-core/src/jobs.rs`
- [ ] **P1** Move event prune / lifecycle maintain / artifact cleanup off
  async request paths (`spawn_blocking` or worker) — `event.rs`,
  `lifecycle.rs`
- [ ] **P1** Drain completed RPC `JoinSet` tasks in accept loop; cap
  concurrent connections — `crates/retcon-core/src/server.rs`
- [ ] **P1** Browser reader cleanup generation ID (don't clear a newly
  started process) — `crates/retcon-core/src/spikes/browser.rs`
- [ ] **P1** Enforce outbound frame size (`MAX_FRAME_BYTES`); align Git
  output cap with protocol (or page large diffs as artifacts) —
  `server.rs`, `retcon-git`, `retcon-protocol`

### P2

- [ ] **P2** `jobs.list` ordered index or SQL pagination (avoid clone+sort
  of full map) — `crates/retcon-core/src/jobs.rs`
- [ ] **P2** Paginate `SessionRepository::list_for_project` —
  `crates/retcon-storage/src/repositories.rs`
- [ ] **P2** Artifact retention: stream/`EXISTS` instead of full hash set —
  `crates/retcon-storage/src/artifacts.rs`
- [ ] **P2** `CREATE INDEX … ON agent_events(created_at)` + scheduled age
  prune (not in emit path) — migrations + `event.rs`
- [ ] **P2** Attach Playwright listeners to all pages (`context.on("page")`)
  — `apps/browser-service/src/browser.ts`
- [ ] **P2** Floating panel instance IDs + `ValueKey` for stateful bodies —
  `apps/desktop/lib/src/workspace.dart`
- [ ] **P2** Serialize/coalesce workspace layout persistence —
  `apps/desktop/lib/src/workspace.dart`
- [ ] **P2** Fixed-index ring buffer for browser logs (vs `splice`) —
  `apps/browser-service/src/browser.ts`
- [ ] **P2** BLOB-native fresh schema (skip TEXT→rebuild on new DBs)

### P3

- [ ] **P3** Website asset generation mtime/hash short-circuit —
  `apps/website/scripts/generate-brand-assets.mjs`
- [ ] **P3** Guard redundant focus `setState` in design-system controls —
  `packages/retcon-design-system/lib/src/components/controls.dart`
- [ ] **P3** CI Playwright cache + path filters
- [ ] **P3** Update storage/core tests for schema v4 UUID BLOBs (fixtures
  still insert TEXT IDs / expect version 3)

---

## Suggested order of attack

1. Job writer actor + prune off emit path (async latency)
2. RPC JoinSet drain + connection cap + outbound frame limit
3. Browser process generation + multi-page listeners
4. Jobs/sessions pagination + event `created_at` index
5. Desktop panel instance IDs + save coalescing
6. Website/CI polish
