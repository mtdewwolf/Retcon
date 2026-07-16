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
- [x] **P0** Cap RPC frame size on read (`crates/retcon-core/src/frame.rs`,
  `server.rs`, `spikes/browser.rs`)
- [x] **P0** Periodic SQLite event retention (`crates/retcon-core/src/event.rs`)
- [x] **P0** Evict finished jobs from memory + DB (`crates/retcon-core/src/jobs.rs`)
- [x] **P0** Graceful browser-service shutdown before kill (`spikes/browser.rs`)
- [x] **P0** Bound Git command output (`crates/retcon-git/src/lib.rs`)
- [x] **P1** Don't hold event-bus mutex across SQLite insert (`event.rs` writer thread)
- [x] **P1** Batch / coalesce terminal output events (`spikes/terminal.rs`)
- [x] **P1** Move SQLite work off Tokio worker threads (`database.rs` `read_async`,
  `artifacts.rs` async helpers)
- [x] **P1** Add status indexes for recovery queries (migration `0003`)
- [x] **P1** Enforce log pagination / tail in `browser.logs` (`browser.ts`)
- [x] **P1** Default screenshot to viewport (`browser.ts`)
- [x] **P1** Combine Git status into one process (`retcon-git/src/lib.rs`)
- [x] **P2** Browser proc lock held across stdin writes — writer actor split (`browser.rs`)
- [x] **P2** Clear stale `BrowserHandle.proc` + drain pending on reader exit (`browser.rs`)
- [x] **P2** Artifact retention hash indexes (migration `0003`)
- [x] **P2** Run artifact disk walks in `spawn_blocking` (`artifacts.rs`)
- [x] **P2** Skip full rehash on duplicate artifact store (`artifacts.rs`)
- [x] **P2** Binary-search event replay (`event.rs`)
- [x] **P2** `Arc<EventEnvelope>` to cut clone cost on emit (`event.rs`)
- [x] **P2** `prepare_cached` for hot statements (`event.rs`, `jobs.rs`)
- [x] **P2** `PRAGMA quick_check` on normal open; full check for diagnostics (`database.rs`)
- [x] **P2** `SQLITE_OPEN_NO_MUTEX` when Rust serializes the connection (`database.rs`)
- [x] **P2** Tune `cache_size` / `mmap_size` / `wal_autocheckpoint` (`database.rs`)
- [x] **P2** Debounce job persistence; avoid Debug-format enum names (`jobs.rs`)
- [x] **P2** Don't hold terminal registry lock while killing (`spikes/terminal.rs`)
- [x] **P2** Cache / async shell detection (`spikes/terminal.rs`)
- [x] **P2** Cache agent/provider detection (`retcon-agents`)
- [x] **P2** Await `child.wait()` instead of 250ms poll (`spikes/agent.rs`)
- [x] **P2** Cache schema version after open (`state.rs`)
- [x] **P2** Lazy-load event replay pages from DB at startup (`event.rs`)
- [x] **P2** Pass browser temp profile dir via `launchPersistentContext` (`browser.ts`)
- [x] **P2** Stdout backpressure on browser-service event emits (`main.ts`)
- [x] **P2** Spawn `bun` directly (no `cmd /C` shell) on Windows (`spikes/browser.rs`)
- [x] **P3** Deduplicate console/network stream vs buffer shipping (`browser.ts` — network
  responses now land in the ring buffer)
- [x] **P3** Method-aware concurrency for browser-service stdio RPC (`main.ts`)
- [x] **P3** Cache log level at module load (`logging.ts`)
- [x] **P3** Website asset generation: single Sharp pipeline + `Promise.all` for icons
- [x] **P3** Website CLS: explicit image dimensions; defer Tally iframe (`index.astro`)
- [x] **P3** CI-generate PWA icons referenced by `site.webmanifest` (`package.json` build)

---

## Deferred (future schema work)

- [ ] **P3** UUID-as-BLOB schema migration to shrink indexes (`repositories.rs`)
  - Requires a coordinated schema v4 migration across every TEXT UUID column and all
    client bindings; deferred until a dedicated migration sprint.

---

## Suggested order of attack (completed)

1. Frame-size caps + event retention + job eviction (bounds) ✅
2. Event writer off async path + terminal coalesce (throughput) ✅
3. Recovery indexes + Git/browser evidence polish (latency) ✅
4. Storage pragma / statement-cache tuning (incremental) ✅
