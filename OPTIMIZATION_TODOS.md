# Optimization TODO list

Full codebase review against Master `d8ee390` (“test dashboard, fixing core”).
Priorities: **P0** = correctness / unbounded growth; **P1** = hot-path performance;
**P2** = medium impact; **P3** = polish / future.

Stub crates with little surface (`retcon-index`, `retcon-diagnostics`, `retcon-updater`,
empty TypeScript packages) are omitted unless noted.

---

## Done in this PR

- [x] **P0** Terminal scrollback: share buffer via `Arc` (clone was mutating a disconnected
  copy), hard-cap in-memory scrollback, bound coalesce channel
  (`crates/retcon-core/src/spikes/terminal.rs`)
- [x] **P0** Bound durable event persist queue (`sync_channel(4096)`), move prune onto writer
  thread, add `EventBus::flush`, cut redundant envelope clones
  (`crates/retcon-core/src/event.rs`)
- [x] **P0** Delete browser-service `recoverable` entry on session close
  (`apps/browser-service/src/browser.ts`)
- [x] **P0** Cap Flutter terminal-view tab output (`packages/retcon-terminal-view`)
- [x] **P1** Cap Git stderr capture (`crates/retcon-git/src/lib.rs`)
- [x] **P1** Bound `browser_history` inserts + limited history reads
  (`crates/retcon-storage/src/browser.rs`)
- [x] **P1** Coalesce conversation stream `notifyListeners` + cap transcript length
  (`apps/desktop/lib/src/conversation/conversation_controller.dart`)
- [x] **P0** Fix Unix transport split types (`OwnedReadHalf`/`OwnedWriteHalf`) so core builds
  on Linux (`crates/retcon-platform/src/transport.rs`)

---

## Open — P0

_(none remaining after this PR)_

---

## Open — P1

- [ ] **Job persistence writer actor** — `jobs.rs` `persist_snapshot` / `flush_persist` still
  runs sync SQLite on Tokio tasks; debounce + dedicated writer like the event bus.
- [ ] **Wire `Database::read_async`** — API exists in `database.rs` but has **zero call sites**;
  hot RPC/repos still block worker threads.
- [ ] **RPC head-of-line blocking** — `server.rs` awaits one dispatch before the next read;
  long `git.clone` / browser / verification stalls the socket (events + other RPCs).
- [ ] **`project.clone` as supervised job** — `projects.rs` runs full `git clone` inside RPC
  with buffered output; should be a `JobSupervisor` job with progress events.
- [ ] **Paginate Wave 3/4 list RPCs** — sessions, tasks, messages, verification,
  browser verification, browser tabs/observations still `collect()` / no `LIMIT`.
- [ ] **Approval fingerprint indexes** — `repositories.rs` uses `json_extract(...fingerprint)`;
  add migration `0012` with stored fingerprint column + indexes (update `lifecycle.rs`
  health assertion past schema v11).
- [ ] **Secrets staged-diff stream/cap** — `retcon-secrets/src/staged.rs` loads full
  `git diff --cached` via sync `Command::output()`.
- [ ] **Filesystem `spawn_blocking`** — `filesystem/service.rs` sync `read_dir`/`read`/`write`
  called from async `file_rpc`.
- [ ] **Desktop N+1 repositories** — tasks / verification / browser verification / browser
  evidence / dev-server refresh do list→get storms; debounce event-driven refresh.
- [ ] **Browser snapshot defaults** — `browser.snapshot` defaults DOM + accessibility `true`
  (up to 1MB each); default false and opt-in for verification.
- [ ] **Windows pipe transport** — desktop `transport.dart` 50Hz poll + sync writes on UI isolate.

---

## Open — P2

- [ ] **Terminal exit: event-driven wait** — still 500ms poll in `spikes/terminal.rs`.
- [ ] **Permission-rule cache** — `permissions/engine.rs` reloads/evaluates rules twice per check.
- [ ] **`projects.list` N+1 + in-memory filter** — load all projects then filter in Rust.
- [ ] **Agent stream line length cap** — `retcon-agents` `BufReader::lines()` uncapped.
- [ ] **File-watch caps** — recursive watches + full-file SHA; cap registry and hash by size/mtime.
- [ ] **Shorter finished-job RAM retention** — 7-day in-memory finished jobs is long; keep last N.
- [ ] **Debounce workspace layout persist** — floating drag / splitter writes every frame
  (`workspace.dart`).
- [ ] **Diff / IDE virtualization** — eager hunk/line builds; tree `ListView` without builder.
- [ ] **Verification / trace capture lean defaults** — fullPage screenshots and trace `sources`.
- [ ] **Reset conversation on project switch** — transcript/`sessionId` can leak across projects.
- [ ] **Browser RPC pending timeouts** — `apps/browser-service/src/rpc.ts` has no request timeout.
- [ ] **Test-dashboard stdout/stderr caps** — `test_runner.dart` `.join()`s full process output;
  discovery sync-walks crates.

---

## Open — P3

- [ ] **Split mega-modules** — `repositories.rs`, browser verification RPC/storage, agents lib,
  browser-service `verification.ts`.
- [ ] **Spawn agent binaries directly** — agents still use `cmd /C` on Windows where safe.
- [ ] **BLOB-native fresh schema** — UUID BLOBs already migrated; simplify greenfield path.
- [ ] **Floating panel drag debounce / syntax highlighter allocs** — file viewer / panels.
- [ ] **Core log panel ring** — `core_log_panel.dart` uses `removeAt(0)` instead of a queue.

---

## Suggested order of attack

1. Job writer + `read_async` on hot RPCs (Tokio unblock)
2. RPC HOL split / long ops as jobs (`project.clone`, verification)
3. Paginate list RPCs + fingerprint migration `0012`
4. Desktop N+1 + refresh debounce + snapshot defaults
5. Filesystem/secrets/file-watch caps; UI virtualization

---

## Notes from this review

- Prior Round-9 items claimed on older agent branches (schema v12, `sync_channel`) had **not**
  landed on Master before this PR; schema remains **v11** until fingerprint migration ships.
- `OPTIMIZATION_TODOS.md` previously listed almost everything as Done while several DONE claims
  were overstated (`read_async` unused; event queue unbounded). This file is now the live backlog.
