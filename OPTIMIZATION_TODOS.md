# Optimization TODO list

Full codebase review against Master head `d3e90f2` (schema v6 / Wave 4 +
task planning + verification). Priorities: **P0** = correctness / unbounded
growth on hot paths; **P1** = hot-path performance / memory bounds;
**P2** = medium impact; **P3** = polish / future.

Stub crates (`retcon-browser`, `retcon-index`, `retcon-diagnostics`,
`retcon-updater`) and empty TypeScript packages have no meaningful
optimization surface yet.

---

## Done in prior reviews (still landed on Master)

- [x] **P0** Cap browser-service console/network buffers with a ring buffer
- [x] **P0** Remove timed-out / closed browser RPC entries from `pending`
- [x] **P0** Cap RPC frame size on read (`frame.rs`, `server.rs`, browser spike)
- [x] **P0** Periodic SQLite event retention
- [x] **P0** Evict finished jobs from memory + DB
- [x] **P0** Graceful browser-service shutdown before kill
- [x] **P0** Bound Git command stdout (`retcon-git`)
- [x] **P1** Don't hold event-bus mutex across SQLite insert (writer thread)
- [x] **P1** Batch / coalesce terminal output events
- [x] **P1** Move SQLite work off Tokio worker threads (`read_async`, artifacts)
- [x] **P1** Recovery / artifact indexes (migrations `0003`+)
- [x] **P1** Browser log pagination / viewport screenshots
- [x] **P1** Combine Git status into one process
- [x] **P2** Browser writer actor; clear stale proc; artifact walks / hash skip
- [x] **P2** Binary-search event replay; `Arc<EventEnvelope>`; `prepare_cached`
- [x] **P2** SQLite pragma / `NO_MUTEX` / cache tuning
- [x] **P2** Debounce job persistence; terminal lock-free kill path (partial)
- [x] **P2** Cache shell / agent / schema version detection
- [x] **P2** Lazy event replay pages; browser profile dir / stdout backpressure
- [x] **P3** Website Sharp pipeline / CLS / PWA icons; UUID-as-BLOB migration

---

## Done in this review PR

- [x] **P1** Store live terminals as `Arc<LiveTerminal>` (no full scrollback clone)
- [x] **P1** Cap in-memory terminal scrollback (`MAX_SCROLLBACK_BYTES`)
- [x] **P1** Fix browser `proc` / `pending` async mutex lock order
- [x] **P1** Kill Git child on stdout overflow; cap stderr like stdout
- [x] **P1** Cap `git apply` patch I/O (bounded stdout/stderr + size check)
- [x] **P1** Cap agent provider stdout/stderr line length
- [x] **P2** Release Windows PTY mutex before `taskkill`
- [x] **P3** `negotiate_features` via `HashSet`

---

## Open — P1 (do next)

- [ ] **P1** Job-persistence writer actor / `spawn_blocking` for `persist_snapshot`
  (`crates/retcon-core/src/jobs.rs`) — supervisor updates still write SQLite
  inline on the async path.
- [ ] **P1** Explicit event-bus `flush()` / barrier + bounded durable queue
  (`crates/retcon-core/src/event.rs`) — `mpsc::channel()` is unbounded; no
  wait-for-persist API for shutdown/tests.
- [ ] **P1** Run `project.clone` as a supervised job with capped streamed progress
  (`crates/retcon-core/src/projects.rs`) — foreground `Command::output()` buffers
  all clone I/O and blocks the RPC.
- [ ] **P1** Checkpoint / file-write: `spawn_blocking` + max artifact size
  (`crates/retcon-checkpoints/src/service.rs`, `crates/retcon-core/src/file_rpc.rs`)
  — full-file `fs::read` / `read_to_end` on async RPC paths.
- [ ] **P1** Split per-connection RPC reader vs request execution
  (`crates/retcon-core/src/server.rs`) — long RPCs block cancels and event
  forwarding (head-of-line blocking).
- [ ] **P1** Index approval fingerprints (`retcon-storage` migration +
  `repositories.rs`) — `json_extract(request_json,'$.fingerprint')` full scan.
- [ ] **P1** Cap desktop transport frame size
  (`apps/desktop/lib/src/transport.dart`) — `LineSplitter` / pipe buffer
  unbounded until newline.
- [ ] **P1** Stream / cap staged secret-scan diffs
  (`crates/retcon-secrets/src/staged.rs`) — `git diff --cached` via `.output()`.

---

## Open — P2

- [ ] **P2** Paginate session / turn / message / checkpoint / task list RPCs
  (`session_rpc.rs`, `checkpoints_rpc.rs`, `tasks_rpc.rs`, `repositories.rs`,
  `task_planning.rs`) — many `.collect()` without SQL `LIMIT`.
- [ ] **P2** SQL-level `project.list` filter + pagination
  (`projects.rs`, `repositories.rs`) — N+1 metadata loads + in-memory filter.
- [ ] **P2** Permission-rule in-memory cache invalidated on mutate
  (`retcon-permissions/src/engine.rs`) — reload from SQLite every guarded RPC.
- [ ] **P2** Cap file watches + prune debounce `last_emit` map
  (`retcon-filesystem/src/watch.rs`).
- [ ] **P2** `spawn_blocking` for filesystem list/read/write
  (`file_rpc.rs`, `retcon-filesystem/src/service.rs`).
- [ ] **P2** Cache / lazy git-status badges for `file.list`
  (`retcon-filesystem/src/service.rs`) — full-repo status per directory list.
- [ ] **P2** Batch task dependency loads; cap plan/evidence/history arrays
  (`task_planning.rs`).
- [ ] **P2** Storage status/recovery off async path (`storage_rpc.rs`).
- [ ] **P2** Cap command-tracker input buffer (`retcon-terminal/command_tracker.rs`).
- [ ] **P2** Conversation UI retention / StringBuffer / notify throttle
  (`apps/desktop/.../conversation_controller.dart`).
- [ ] **P2** Terminal-view UI scrollback retention
  (`packages/retcon-terminal-view`).
- [ ] **P2** Paginate checkpoint file-changes / preview; avoid auto-select-all
  (`checkpoint_controller.dart`, checkpoint service).
- [ ] **P2** Clamp desktop / browser-service NDJSON frame size; fix pending leak
  on write failure (`browser-service` `rpc.ts` / `main.ts`).
- [ ] **P2** Cap `browser.action` `innerText` responses (`browser.ts`).
- [ ] **P2** Cap verification manifest discovery + parser location dedupe
  (`retcon-verification`).
- [ ] **P2** Paginate / clamp `task.list` details fan-out
  (`core_task_repository.dart`).
- [ ] **P2** Timeout + bound Claude doctor/version `.output()` checks
  (`retcon-agents`).

---

## Open — P3

- [ ] **P3** BLOB-native fresh schema (skip TEXT→BLOB migration on new DBs)
- [ ] **P3** Split mega-modules (`repositories.rs`, `task_planning.rs`)
- [ ] **P3** Floating panel instance IDs + debounce persist-on-drag
  (`workspace.dart`)
- [ ] **P3** Diff viewer virtualization (`retcon-diff-viewer`)
- [ ] **P3** Filesystem list `sort_by_cached_key`; precompute dirty-dir badges
- [ ] **P3** Cap worktree list APIs; agent capability `HashSet`
- [ ] **P3** Secrets scan size caps / entropy offset scan
- [ ] **P3** Platform transport: async `remove_file` / `create_dir_all`
- [ ] **P3** Website / CI polish leftovers

---

## Suggested order of attack

1. Job writer actor + event flush/bounded queue (durability under load)
2. Checkpoint/file `spawn_blocking` + size caps (OOM / runtime stalls)
3. `project.clone` supervised job + RPC HOL split (responsiveness)
4. SQL pagination + approval fingerprint index (list RPC scaling)
5. Desktop/browser frame caps + UI retention (client memory)
6. Permission cache, file-watch caps, filesystem git-status cache
7. P3 polish (schema, module splits, virtualization)
