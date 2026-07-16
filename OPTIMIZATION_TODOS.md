# Optimization TODO list

Full codebase review after project repository queries / RPC routing
(`aaacba9`), plus unmerged hot-path fixes from earlier review rounds.
Priorities: **P0** = correctness / unbounded growth; **P1** = hot-path
performance; **P2** = medium impact; **P3** = polish / future.

Stub crates (`retcon-worktrees`, `retcon-browser`, `retcon-checkpoints`,
`retcon-filesystem`, `retcon-index`, `retcon-platform`, `retcon-permissions`,
`retcon-diagnostics`, `retcon-updater`) and empty TypeScript/Dart packages
have no meaningful optimization surface yet.

---

## Done in this PR

### From project RPC commit (`aaacba9`)

- [x] **P0** Cap `project.clone` / project Git helpers via `retcon_git`
  (no unbounded `.output()`) — `crates/retcon-core/src/projects.rs`
- [x] **P1** Avoid full `git status --porcelain` in health; use
  `rev-parse` + `diff --quiet` + capped `ls-files` —
  `projects.rs`
- [x] **P1** Run filesystem `analyze()` in `spawn_blocking` —
  `projects.rs`
- [x] **P1** Bound `project.list` (default 50 / max 200) + batch metadata
  via `settings.list_by_key` (no N+1) — `projects.rs`, `repositories.rs`,
  `projects_rpc.rs`
- [x] **P1** Index active projects + settings-by-key (migration `0005`)
- [x] **P3** Call `add_location` once on open (no duplicate upsert)

### Carried from prior unmerged review work

- [x] **P0** Cap RPC frame reads with chunked `fill_buf` (no unbounded
  `read_until`) — `crates/retcon-core/src/frame.rs`
- [x] **P0** Cap Git stderr; kill child on stdout/stderr overflow —
  `crates/retcon-git/src/lib.rs`
- [x] **P0** Bound event persist queue (`sync_channel` + `try_send`) —
  `crates/retcon-core/src/event.rs`
- [x] **P1** Drain completed RPC `JoinSet` tasks; cap concurrent
  connections (64) — `crates/retcon-core/src/server.rs`
- [x] **P1** Enforce outbound frame size (`MAX_FRAME_BYTES`) —
  `server.rs`

---

## Still open

### P1

- [ ] **P1** Dedicated job-persistence writer actor (replace sync DB writes
  on job update/finish) — `crates/retcon-core/src/jobs.rs`
- [ ] **P1** Move event prune / lifecycle maintain / artifact cleanup off
  async request & emit paths (`spawn_blocking` or writer-side prune) —
  `event.rs`, `lifecycle.rs`
- [ ] **P1** Browser reader cleanup generation ID (don't clear a newly
  started process) — `crates/retcon-core/src/spikes/browser.rs`
- [ ] **P1** Align Git 4MiB output cap with 1MiB protocol frame (page large
  diffs as artifacts / lower per-method caps) — `retcon-git`, spikes/git
- [ ] **P1** Run `project.clone` as a supervised background job with
  progress events (RPC currently waits for the full clone)

### P2

- [ ] **P2** `jobs.list` SQL-backed ordered pagination (not in-memory
  collect+sort) — `crates/retcon-core/src/jobs.rs`
- [ ] **P2** Paginate `SessionRepository::list_for_project` —
  `crates/retcon-storage/src/repositories.rs`
- [ ] **P2** Artifact retention: stream/`EXISTS` instead of full hash set —
  `crates/retcon-storage/src/artifacts.rs`
- [ ] **P2** `CREATE INDEX … ON agent_events(created_at)` + age prune off
  emit path — migrations + `event.rs`
- [ ] **P2** Push `project.list` name/path filter into SQL (still filters
  in Rust after the limited fetch)
- [ ] **P2** Wrap `add_location` upsert + project `updated_at` in one
  transaction — `repositories.rs`
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

---

## Suggested order of attack

1. Job writer actor + prune/maintain off async paths
2. Browser process generation ID + Git/protocol size alignment
3. `project.clone` as a supervised job
4. Jobs/sessions pagination + event `created_at` index
5. Desktop panel instance IDs + save coalescing
6. Website/CI polish
