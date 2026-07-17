# Optimization TODO list

Full codebase review after session recovery (`a1e7eb3`: mark recovered sessions
as `disconnected` / turns as `failed`).

Priorities: **P0** = correctness / unbounded growth; **P1** = hot-path performance;
**P2** = medium impact; **P3** = polish / future.

Stub crates (`retcon-worktrees`, `retcon-browser`, `retcon-checkpoints`,
`retcon-filesystem`, `retcon-index`, `retcon-platform`, `retcon-permissions`,
`retcon-diagnostics`, `retcon-updater`) and empty TypeScript packages have no
meaningful optimization surface yet.

---

## Done in this PR (post-`a1e7eb3` review)

- [x] **P0** True chunked RPC frame cap via `fill_buf`/`consume`
  (`crates/retcon-core/src/frame.rs`)
- [x] **P0** Git kill-on-overflow + capped stderr
  (`crates/retcon-git/src/lib.rs`)
- [x] **P1** `jobs.shutdown` collects active IDs from the map (bypass 500 page cap)
  (`crates/retcon-core/src/jobs.rs`)
- [x] **P1** Allow `Queued -> Failed` for process-restart recovery
  (`crates/retcon-core/src/session_engine.rs`)
- [x] **P1** Migration `0005`: expand recovery partial indexes + normalize
  legacy `interrupted` rows (`crates/retcon-storage`)
- [x] **P1** Bounded event persist queue (`sync_channel`/`try_send`) + prune on writer
  (`crates/retcon-core/src/event.rs`)
- [x] **P1** Bounded terminal output channel (`try_send`)
  (`crates/retcon-core/src/spikes/terminal.rs`)
- [x] **P1** Browser reader generation ID (don't clear a newer proc)
  (`crates/retcon-core/src/spikes/browser.rs`)
- [x] **P1** Project clone/health via capped `retcon_git`; `analyze` in `spawn_blocking`
  (`crates/retcon-core/src/projects.rs`)
- [x] **P1** RPC connection cap (64) + outbound frame size guard
  (`crates/retcon-core/src/server.rs`)
- [x] **P1** Browser-service: concurrent status/logs, frame caps, intentional-close,
  text/url truncation, logs default to tail
  (`apps/browser-service`)
- [x] **P2** Artifact `UNION ALL` + HashSet dedupe
  (`crates/retcon-storage/src/artifacts.rs`)
- [x] **P2** Claude doctor diagnostics timeouts
  (`crates/retcon-agents/src/lib.rs`)

---

## Remaining TODO

### P1

- [ ] **Dedicated job-persistence writer actor** — debounce/flush off the async
  emit path; avoid `block_in_place` assumptions on current-thread runtimes
  (`crates/retcon-core/src/jobs.rs`)
- [ ] **Event bus flush on shutdown** — join/drain the persist writer so durable
  lifecycle/recovery events are not lost on fast exit
  (`crates/retcon-core/src/event.rs`, `lifecycle.rs`)
- [ ] **`project.clone` as a supervised cancellable job** — long clones should
  report progress and support cancel (`projects.rs` + `jobs.rs`)
- [ ] **Agent stdout/stderr line caps** — `BufReader::lines()` can still buffer an
  arbitrarily long provider line (`crates/retcon-agents`, `spikes/agent.rs`)

### P2

- [ ] **`jobs.list` / session list SQL pagination** — avoid loading large
  in-memory snapshots for API callers
- [ ] **SQL-side `project.list` filter + limit** — currently filters in Rust after
  loading all projects/settings
- [ ] **`agent_events(created_at)` index** — speeds retention deletes when the
  table grows
- [ ] **Artifact retention streaming** — walk/cleanup without materializing the
  full referenced-hash set when possible
- [ ] **Desktop `CoreClient` frame cap** — LineSplitter / unguarded `jsonDecode`
  (`apps/desktop/lib/src/core_client.dart`)
- [ ] **Floating panel instance IDs + closed-panel cap + layout save coalesce**
  (`apps/desktop/lib/src/desktop_shell.dart`, `workspace.dart`)
- [ ] **Windows PTY `taskkill` off the child mutex**
  (`crates/retcon-terminal/src/lib.rs`)
- [ ] **Multi-page Playwright listeners** — only the first page gets
  console/network hooks today (`apps/browser-service/src/browser.ts`)

### P3

- [ ] **`negotiate_features` de-dupe / HashSet lookup**
  (`crates/retcon-protocol/src/lib.rs`)
- [ ] **Website asset short-circuit when outputs are fresh**
- [ ] **CI Playwright browser cache**
- [ ] **BLOB-native fresh schema** — skip v1→v4 rebuild path for new installs
- [ ] **Design-system focus `setState` guard**

---

## Previously completed (earlier review rounds)

- Frame-size intent, event retention, job eviction, browser ring buffers
- Terminal output coalesce, SQLite off hot path, recovery indexes (v3)
- UUID-as-BLOB migration (v4), storage pragma tuning, statement caches
- Browser writer actor split, artifact hash indexes, Git stdout cap (pre-kill)

---

## Suggested order of attack (remaining)

1. Event flush on shutdown + job writer actor (durability / hot path)
2. Agent line caps + `project.clone` job (bounds / UX)
3. Desktop frame caps + floating panel identity (shell stability)
4. SQL pagination / indexes (scale)
