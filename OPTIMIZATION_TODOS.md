# Optimization TODO list

Full codebase review against Master head `329da09` (schema **v12** candidate /
Phases 21–27: verification, dev-servers, managed browser, browser verification,
diagnostics, IDE sync). Priorities: **P0** = correctness / unbounded growth;
**P1** = hot-path performance / memory bounds; **P2** = medium impact;
**P3** = polish / future.

Stub crates (`retcon-index`, `retcon-diagnostics`, `retcon-updater`) and empty
TypeScript packages have little optimization surface.

---

## Done in prior reviews (still on Master)

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
- [x] **P2–P3** Storage pragma tuning, statement cache, UUID-as-BLOB, website CLS

---

## Done in this review PR

- [x] **P1** Store live terminals as `Arc<LiveTerminal>` + cap in-memory scrollback
- [x] **P1** Fix browser spike `proc` / `pending` async mutex lock order
- [x] **P1** Kill Git child on stdout/stderr overflow; cap `git apply` I/O
- [x] **P1** Cap agent provider stdout/stderr line length
- [x] **P2** Release Windows PTY mutex before `taskkill`
- [x] **P3** Unix transport `OwnedReadHalf` / `OwnedWriteHalf` typing
- [x] **P1** Bound durable event persist queue (`sync_channel`) + `EventBus::flush`
- [x] **P1** Approval fingerprint + permission-rule indexes (migration `0012`)
- [x] **P1** Cap desktop transport frame size (`transport.dart`)
- [x] **P2** Cap closed-panel history (32) in workspace layout
- [x] **P1** Browser-service bounded NDJSON frame reader (`framed.ts`)
- [x] **P1** Cap managed browser sessions; drop recoverable on close
- [x] **P2** Clear browser-service RPC `pending` on write failure

---

## Open — P1 (do next)

- [ ] **P1** Job-persistence writer actor / `spawn_blocking` for `persist_snapshot`
  (`crates/retcon-core/src/jobs.rs`) — supervisor updates still write SQLite
  inline on the async path.
- [ ] **P1** Run `project.clone` as a supervised job with capped streamed progress
  (`crates/retcon-core/src/projects.rs`) — foreground `Command::output()` buffers
  all clone I/O and blocks the RPC.
- [ ] **P1** Checkpoint / file-write: `spawn_blocking` + max artifact size
  (`crates/retcon-checkpoints/src/service.rs`, `crates/retcon-core/src/file_rpc.rs`)
  — full-file `fs::read` / `read_to_end` on async RPC paths.
- [ ] **P1** Split per-connection RPC reader vs request execution
  (`crates/retcon-core/src/server.rs`) — long RPCs block cancels and event
  forwarding (head-of-line blocking).
- [ ] **P1** Stream / cap staged secret-scan diffs
  (`crates/retcon-secrets/src/staged.rs`) — `git diff --cached` via `.output()`.
- [ ] **P1** Conversation UI retention / streaming StringBuffer caps
  (`apps/desktop/.../conversation_controller.dart`) — unbounded message/tool text.
- [ ] **P1** Terminal-view UI scrollback ring + virtualization
  (`packages/retcon-terminal-view`) — unbounded `output += data` + full reparse.

---

## Open — P2

- [ ] **P2** Paginate session / turn / message / checkpoint / task / browser /
  verification / browser-verification / dev-server list RPCs — many unbounded
  `.collect()` surfaces added in Phases 21–25.
- [ ] **P2** SQL-level `project.list` filter + pagination (`projects.rs`)
- [ ] **P2** Permission-rule in-memory cache invalidated on mutate
- [ ] **P2** Cap file watches + prune debounce pending map
  (`retcon-filesystem/src/watch.rs`)
- [ ] **P2** `spawn_blocking` for filesystem list/read/write; cache git-status
  badges for `file.list`
- [ ] **P2** Route repository-heavy RPCs through `Database::read_async`
- [ ] **P2** Cap `retcon-browser` Node transport in-flight `pending` map
- [ ] **P2** Debounce desktop browser/dev-server/verification refreshes; pass
  server-side list limits (`core_*_repository.dart`)
- [ ] **P2** Diff viewer byte/line caps + virtualized hunk rendering
- [ ] **P2** IDE / file-explorer directory listing caps + flattened virtualization
- [ ] **P2** Bound terminal coalesce channel (`mpsc::unbounded_channel` in spike)
- [ ] **P2** Cap verification manifest discovery + parser location dedupe
- [ ] **P2** Timeout + bound Claude doctor/version `.output()` checks

---

## Open — P3

- [ ] **P3** BLOB-native fresh schema (skip TEXT→BLOB migration on new DBs)
- [ ] **P3** Split mega-modules (`repositories.rs`, `task_planning.rs`)
- [ ] **P3** Floating panel instance IDs + debounce persist-on-drag
- [ ] **P3** Syntax highlighter avoid `substring` scan allocations
- [ ] **P3** Cap worktree list APIs; agent capability `HashSet`
- [ ] **P3** Secrets scan size caps / entropy offset scan
- [ ] **P3** Platform transport: async `remove_file` / `create_dir_all`
- [ ] **P3** Task list query shape matching `(project_id, status, priority, updated_at)`

---

## Suggested order of attack

1. Checkpoint/file `spawn_blocking` + size caps (OOM / runtime stalls)
2. Job writer actor + `project.clone` supervised job (durability / responsiveness)
3. RPC HOL split (cancel + event delivery under load)
4. SQL pagination across Phase 21–25 list surfaces
5. Desktop/UI retention (conversation, terminal-view, diff, file explorer)
6. Permission cache, file-watch caps, filesystem git-status cache
7. P3 polish (schema, module splits, virtualization)
