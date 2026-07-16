# Optimization TODO list

Full codebase review after the SQLite event-bus migration (`7915613`).
Priorities: **P0** = correctness / unbounded growth; **P1** = hot-path performance;
**P2** = medium impact; **P3** = polish / future.

Stub crates (`retcon-worktrees`, `retcon-browser`, `retcon-checkpoints`,
`retcon-filesystem`, `retcon-index`, `retcon-platform`, `retcon-permissions`,
`retcon-diagnostics`, `retcon-updater`) and empty TypeScript packages have no
meaningful optimization surface yet.

---

## Done in this PR

- [x] **P0** Cap browser-service console/network buffers with a ring buffer
  (`apps/browser-service/src/browser.ts`)
- [x] **P0** Remove timed-out / closed browser RPC entries from `pending`
  (`crates/retcon-core/src/spikes/browser.rs`)

---

## P0 — Correctness / unbounded growth

- [ ] **Cap RPC frame size on read**
  - Where: `crates/retcon-core/src/server.rs` (`BufReader::lines`), browser stdout readers
  - Issue: Protocol defines `MAX_FRAME_BYTES` (`retcon-protocol`) but readers have no limit
  - Fix: Cap with `LinesCodec` / `read_until` + reject oversized frames
  - Impact: Prevents unbounded allocation from a local client or runaway service

- [ ] **Periodic SQLite event retention**
  - Where: `crates/retcon-core/src/event.rs` (`EventBus::open` / `emit`)
  - Issue: DB prune runs only at startup; heartbeats every 5s grow `agent_events` forever
  - Fix: Opportunistic prune every N inserts, or make heartbeats non-durable
  - Impact: Bounds disk growth for long-running cores

- [ ] **Evict finished jobs from memory + DB**
  - Where: `crates/retcon-core/src/jobs.rs`, `background_jobs` table
  - Issue: Completed jobs stay in the supervisor map and SQLite indefinitely
  - Fix: Retention by `finished_at`, paginated `jobs.list`
  - Impact: Prevents unbounded supervisor / table growth

- [ ] **Graceful browser-service shutdown before kill**
  - Where: `crates/retcon-core/src/spikes/browser.rs` `stopService` / `shutdown`
  - Issue: Hard `kill()` skips `browser.close`; Chromium / temp dirs can leak
  - Fix: Best-effort `browser.close` with short timeout, then kill; drain pending
  - Impact: Avoids zombie Chromium and `/tmp/retcon-browser-*` leaks

- [ ] **Bound Git command output**
  - Where: `crates/retcon-git/src/lib.rs` (`Command::output`, especially `diff`)
  - Issue: Full stdout capture can allocate huge buffers for large diffs
  - Fix: Byte limits + stream oversized output into artifacts
  - Impact: Protects core/RPC memory on large repos

---

## P1 — Hot-path performance

- [ ] **Don't hold event-bus mutex across SQLite insert**
  - Where: `crates/retcon-core/src/event.rs` `EventBus::emit`
  - Issue: Sync DB write under mutex blocks replay/latest and Tokio workers
  - Fix: Reserve sequence under lock → persist via `spawn_blocking` / writer task → append
  - Impact: High under terminal/browser event load

- [ ] **Batch / coalesce terminal output events**
  - Where: `crates/retcon-core/src/spikes/terminal.rs` PTY output callback
  - Issue: Every chunk sync-emits + persists; many tiny SQLite writes
  - Fix: Size/time coalesce; batch inserts in one transaction
  - Impact: High for chatty PTY sessions

- [ ] **Move SQLite work off Tokio worker threads**
  - Where: `crates/retcon-storage/src/database.rs` + all async callers
  - Issue: Single `Mutex<Connection>` used from async RPC paths
  - Fix: `spawn_blocking` or dedicated storage actor; read pool later if needed
  - Impact: High once concurrent RPC + events compete

- [ ] **Add status indexes for recovery queries**
  - Where: `crates/retcon-storage/src/recovery.rs`, `migrations/0001_initial.sql`
  - Issue: Recovery filters by `status` but most indexes lead with `session_id`
  - Fix: Partial indexes on active statuses (sessions, turns, terminals, browsers, approvals, tasks)
  - Impact: Faster crash recovery as tables grow

- [ ] **Enforce log pagination / tail in `browser.logs`**
  - Where: `apps/browser-service/src/browser.ts`
  - Issue: Even with a ring buffer, full arrays go over stdio JSON-RPC
  - Fix: `{ offset, limit }` or last-N API
  - Impact: Smaller RPC payloads under noisy pages

- [ ] **Default screenshot to viewport (not `fullPage`)**
  - Where: `apps/browser-service/src/browser.ts` `screenshot`
  - Issue: Full-page stitch is expensive on long pages
  - Fix: Viewport default; explicit `fullPage` / JPEG quality params
  - Impact: Faster evidence captures, lower peak memory

- [ ] **Combine Git status into one process**
  - Where: `crates/retcon-git/src/lib.rs` `status`
  - Issue: Two `git` invocations (branch + porcelain)
  - Fix: `git status --porcelain=v2 --branch`
  - Impact: Half the process spawn cost on status polls

---

## P2 — Medium

- [ ] **Browser proc lock held across stdin writes** — split writer actor from process state (`spikes/browser.rs`)
- [ ] **Clear stale `BrowserHandle.proc` + drain pending on reader exit** (`spikes/browser.rs`)
- [ ] **Artifact retention `UNION` over unindexed hash columns** — index hashes or maintain `artifact_references` (`artifacts.rs`)
- [ ] **Run artifact disk walks in `spawn_blocking`** / cache usage (`artifacts.rs`, diagnostics)
- [ ] **Skip full rehash on duplicate artifact store** for the common path (`artifacts.rs`)
- [ ] **Binary-search / partition event replay** instead of linear scan (`event.rs`)
- [ ] **`Arc<EventEnvelope>` (or metadata split) to cut double-clone on emit** (`event.rs`)
- [ ] **`prepare_cached` for hot statements** (event insert, job upsert) (`database.rs` / repositories)
- [ ] **`PRAGMA quick_check` on normal open; full check for diagnostics** (`database.rs`)
- [ ] **`SQLITE_OPEN_NO_MUTEX`** when Rust already serializes the connection (`database.rs`)
- [ ] **Tune `cache_size` / `mmap_size` / `wal_autocheckpoint`** for local workload (`database.rs`)
- [ ] **Debounce job persistence; avoid Debug-format enum names** (`jobs.rs`)
- [ ] **Don't hold terminal registry lock while killing** (`spikes/terminal.rs`, `retcon-terminal`)
- [ ] **Cache / async shell detection** (`spikes/terminal.rs` `detectShells`)
- [ ] **Cache agent/provider detection** (`retcon-agents`)
- [ ] **Await `child.wait()` instead of 250ms poll** (`spikes/agent.rs`)
- [ ] **Cache schema version after open** (`state.rs` `health`)
- [ ] **Lazy-load event replay pages from DB** instead of hydrating all retained events at startup (`event.rs`)
- [ ] **Pass or remove unused browser temp profile dir** (`browser.ts` `mkdtemp` unused by Playwright launch)
- [ ] **Stdout backpressure** on browser-service event emits (`main.ts`)
- [ ] **Spawn `bun` directly** (no `cmd /C` shell) on Windows (`spikes/browser.rs`)

---

## P3 — Low / future

- [ ] UUID-as-BLOB schema migration to shrink indexes (`repositories.rs`)
- [ ] Deduplicate console/network stream vs buffer shipping (`browser.ts`)
- [ ] Method-aware concurrency for browser-service stdio RPC (`main.ts`)
- [ ] Cache log level at module load (`logging.ts`)
- [ ] Website asset generation: single Sharp pipeline + `Promise.all` for icons
- [ ] Website CLS: explicit image dimensions; defer Tally iframe
- [ ] Commit or CI-generate PWA icons referenced by `site.webmanifest`

---

## Suggested order of attack

1. Frame-size caps + event retention + job eviction (bounds)
2. Event writer off async path + terminal coalesce (throughput)
3. Recovery indexes + Git/browser evidence polish (latency)
4. Storage pragma / statement-cache tuning (incremental)
