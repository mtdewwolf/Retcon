# Optimization TODO list

Full codebase review after Wave 3 (`2edeb8c`: sessions, permissions,
checkpoints, filesystem, desktop conversation shell, protocol codegen).

Priorities: **P0** = correctness / unbounded growth; **P1** = hot-path performance;
**P2** = medium impact; **P3** = polish / future.

Stub crates (`retcon-browser`, `retcon-index`, `retcon-diagnostics`,
`retcon-updater`) and placeholder packages still have little optimization
surface. Newly substantial crates (`retcon-filesystem`, `retcon-checkpoints`,
`retcon-permissions`, `retcon-platform`, `retcon-secrets`) are in scope.

---

## Done in this PR (post-`2edeb8c` review)

- [x] **P0** True chunked RPC frame cap via `fill_buf`/`consume`
  (`crates/retcon-core/src/frame.rs`)
- [x] **P0** Git kill-on-overflow + capped stdout/stderr + capped `git apply`
  (`crates/retcon-git/src/lib.rs`)
- [x] **P0** Bound terminal output channel + clear/cap scrollback on flush
  (`crates/retcon-core/src/spikes/terminal.rs`)
- [x] **P1** Bounded event persist queue (`sync_channel`/`try_send`) + prune on writer
  (`crates/retcon-core/src/event.rs`)
- [x] **P1** RPC connection cap (64) + outbound frame size guard
  (`crates/retcon-core/src/server.rs`)
- [x] **P1** `jobs.shutdown` collects active IDs from the map (bypass 500 page cap)
  (`crates/retcon-core/src/jobs.rs`)
- [x] **P1** Allow `Queued -> Failed` for process-restart recovery
  (`crates/retcon-core/src/session_engine.rs`)
- [x] **P1** Migration `0005`: recovery indexes + `agent_events(created_at)` +
  normalize legacy `interrupted` rows (`crates/retcon-storage`)
- [x] **P1** Browser reader generation ID (don't clear a newer proc)
  (`crates/retcon-core/src/spikes/browser.rs`)
- [x] **P1** Project clone/health via capped `retcon_git`; `analyze` in `spawn_blocking`
  (`crates/retcon-core/src/projects.rs`)
- [x] **P1** Agent stdout/stderr line caps + Claude doctor timeouts
  (`crates/retcon-agents/src/lib.rs`)
- [x] **P1** Browser-service: multi-page listeners, frame caps, intentional-close,
  text/url truncation, logs default to tail (`apps/browser-service`)
- [x] **P1** Clamp `file.read` / `file.write` limits server-side
  (`crates/retcon-core/src/file_rpc.rs`)
- [x] **P2** Artifact `UNION ALL` + HashSet dedupe
  (`crates/retcon-storage/src/artifacts.rs`)
- [x] **P2** Desktop transport inbound frame cap
  (`apps/desktop/lib/src/transport.dart`)
- [x] **P2** Cap closed-panel history at 32
  (`apps/desktop/lib/src/workspace.dart`)
- [x] **P2** Clamp `approval.list` limit
  (`crates/retcon-core/src/permissions_rpc.rs`)

---

## Remaining TODO

### P1

- [ ] **Dedicated job-persistence writer actor** — debounce/flush off the async
  emit path; avoid `block_in_place` assumptions on current-thread runtimes
  (`crates/retcon-core/src/jobs.rs`)
- [ ] **Explicit event-bus flush API on shutdown** — Drop joins the writer today,
  but lifecycle should call an explicit flush before teardown
  (`event.rs`, `lifecycle.rs`)
- [ ] **`project.clone` as a supervised cancellable job** — long clones should
  report progress and support cancel (`projects.rs` + `jobs.rs`)
- [ ] **Filesystem / checkpoint RPC `spawn_blocking`** — `file.*` and checkpoint
  hooks still do sync disk IO on the async request path
  (`file_rpc.rs`, `retcon-filesystem`, `retcon-checkpoints`)
- [ ] **Checkpoint size caps / background jobs** — `turn.send` and mutating Git
  can synchronously snapshot large files (`checkpoints/src/service.rs`)

### P2

- [ ] **Session / message / turn / checkpoint list SQL pagination** — Wave 3 list
  RPCs still return unbounded `Vec`s (`session_rpc.rs`, `checkpoints_rpc.rs`,
  `repositories.rs`)
- [ ] **SQL-side `project.list` filter + limit** — currently filters in Rust after
  loading all projects/settings (`projects.rs`)
- [ ] **Approval fingerprint column / expression index** — avoid
  `json_extract` scans on every protected RPC (`repositories.rs`)
- [ ] **Permission-rule cache with invalidation** — rules reload on each protected
  call (`permissions/engine.rs`)
- [ ] **File-watch registry caps + debounce TTL prune**
  (`retcon-filesystem/src/watch.rs`)
- [ ] **`storage.status` background cache** — integrity/maintenance should not run
  on every status RPC (`storage_rpc.rs`)
- [ ] **Secret-scan request byte/count caps**
  (`secrets_rpc.rs`, `retcon-secrets`)
- [ ] **Floating panel instance IDs + layout-save debounce on drag**
  (`apps/desktop/lib/src/workspace.dart`)
- [ ] **Conversation stream buffer + transcript retention cap**
  (`conversation_controller.dart`)
- [ ] **Flutter terminal bounded scrollback + incremental ANSI parse**
  (`packages/retcon-terminal-view`)
- [ ] **Windows PTY `taskkill` off the child mutex**
  (`crates/retcon-terminal/src/lib.rs`)

### P3

- [ ] **Diff viewer virtualization / preview cap**
  (`packages/retcon-diff-viewer`)
- [ ] **File explorer `ListView.builder` for expanded trees**
  (`packages/retcon-file-viewer`)
- [ ] **`negotiate_features` de-dupe / HashSet lookup**
  (`crates/retcon-protocol/src/lib.rs`)
- [ ] **Website asset short-circuit when outputs are fresh**
- [ ] **CI Playwright browser cache**
- [ ] **BLOB-native fresh schema** — skip v1→v4 rebuild path for new installs
- [ ] **Split mega-modules** — `repositories.rs` (~1.9k), `session_rpc.rs` (~1k),
  `workspace.dart` / `desktop_shell.dart` for maintainability

---

## Previously completed (earlier review rounds)

- Frame-size intent, event retention, job eviction, browser ring buffers
- Terminal output coalesce, SQLite off hot path, recovery indexes (v3)
- UUID-as-BLOB migration (v4), storage pragma tuning, statement caches
- Browser writer actor split, artifact hash indexes, Git stdout cap (pre-kill)

---

## Suggested order of attack (remaining)

1. Filesystem/checkpoint `spawn_blocking` + checkpoint size caps (Wave 3 hot path)
2. Job writer actor + explicit event flush (durability)
3. SQL pagination for sessions/messages/checkpoints (scale)
4. Desktop conversation/terminal retention (UI memory)
5. Permissions fingerprint index + rule cache (auth hot path)
