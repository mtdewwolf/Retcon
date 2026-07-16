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

## Completed schema work

- [x] **P3** UUID-as-BLOB schema migration (`retcon-storage` migration `0004`)
  - Rebuilds v1-v3 databases transactionally, converts every UUID column to compact
    16-byte BLOBs, recreates indexes, and updates active repository/core bindings.

---

## Suggested order of attack (completed)

1. Frame-size caps + event retention + job eviction (bounds) ✅
2. Event writer off async path + terminal coalesce (throughput) ✅
3. Recovery indexes + Git/browser evidence polish (latency) ✅
4. Storage pragma / statement-cache tuning (incremental) ✅

---

## Remaining — non-Rust (2026-07-16 review)

Prior Done items above are still closed unless noted. Stub Flutter packages
(`retcon-diff-viewer`, `retcon-file-viewer`, `retcon-plugin-sdk`,
`retcon-terminal-view`) and empty `scripts/` / `examples/` / `tests/` trees
have no optimization surface yet.

### Incomplete from prior “Done”

- [ ] **P1** Method-aware stdio concurrency is ineffective
  (`apps/browser-service/src/main.ts` — `serveStdio`)
  - `for await` awaits each handler (including navigate/screenshot) before
    reading the next stdin line, so `browser.status` / `browser.logs` cannot
    run while a mutating call is in flight despite `activeMutating`.
  - Fix: parse lines on a reader loop; dispatch mutating work to a single-flight
    queue and allow overlapping read-only handlers (or reply immediately from
    a non-blocking path).

### P0 — bounds / long-running memory

- [ ] **P0** Cap per-entry console/network payload size
  (`apps/browser-service/src/browser.ts` — console/request/response handlers)
  - Ring buffer caps *count* (1000) but not *bytes*. `message.text()` and long
    URLs can make each slot multi‑MB → multi‑GB RSS over a busy session.
  - Fix: truncate text/URL fields (e.g. 2–8 KiB) before `pushCapped` / emit;
    optionally track approximate buffer bytes.

- [ ] **P0** Cap stdio RPC request line size before `JSON.parse`
  (`apps/browser-service/src/main.ts` — `serveStdio`)
  - `createInterface` + `JSON.parse(line)` accepts unbounded lines. Core now
    caps frames; the browser-service stdin path does not.
  - Fix: reject/drop lines over a fixed max (aligned with core frame limit)
    before parse.

### P1 — hot path / CI wall time

- [ ] **P1** Clear log buffers on `close()` / failed relaunch
  (`apps/browser-service/src/browser.ts` — `close`)
  - `consoleEntries` / `networkEntries` survive `close()`; relaunch on the same
    `ManagedBrowser` retains prior evidence and RAM.
  - Fix: clear both arrays (and detach page listeners) in `close()`.

- [ ] **P1** Sample or coalesce high-frequency network/console emits
  (`apps/browser-service/src/browser.ts` + `main.ts` — `emit` / `emitEvent`)
  - Every request/response/console line builds objects, ring-pushes, and tries
    stdout JSON write (backlog 64 drops silently). Busy pages burn CPU even
    when events are dropped.
  - Fix: always buffer locally; throttle/sample live `browser.request` /
    `browser.response` streams (or emit summaries); count drops.

- [ ] **P1** Cache Playwright Chromium in CI
  (`.github/workflows/ci.yml` — `browser-service` job)
  - `bunx playwright install --with-deps chromium` runs cold every job.
  - Fix: `actions/cache` on `~/.cache/ms-playwright` keyed by Playwright
    version / `bun.lock`; keep `--with-deps` only on cache miss if possible.

- [ ] **P1** Path-filter CI jobs / avoid full Windows rebuild on unrelated PRs
  (`.github/workflows/ci.yml`)
  - Every PR runs rust + flutter + browser-service + website + Windows release
    build with no `paths` / `paths-ignore`.
  - Fix: path filters (or a lightweight required set + conditional Windows);
    don’t block website-only changes on `windows-build`.

- [ ] **P1** Stabilize Flutter app root ownership
  (`apps/desktop/lib/main.dart` — `RetconApp`)
  - Stateless `build` creates `GoRouter`, `buildLunaDarkTheme()`, and
    `CoreClient()` when `core == null` (never disposed). Fine for one-shot
    `runApp` today; leaks / resets routing on any rebuild (tests use
    `const RetconApp()`).
  - Fix: `StatefulWidget` — own client + router in `initState`/`dispose`;
    cache theme; make `initLogging` idempotent.

### P2 — medium impact

- [ ] **P2** Replace `splice`-based `pushCapped` with index ring buffer
  (`apps/browser-service/src/browser.ts` — `pushCapped`)
  - Overflow does `splice(0, n)` (O(n) copy) on every excess push under load.
  - Fix: fixed array + head/length; slice for `logs()` pagination.

- [ ] **P2** Clean up profile dir if `launchPersistentContext` fails
  (`apps/browser-service/src/browser.ts` — `launch`)
  - `mkdtemp` then launch; throw leaves temp profile on disk.
  - Fix: try/finally remove profile when context was not established.

- [ ] **P2** Attach console/network listeners for all pages
  (`apps/browser-service/src/browser.ts` — `launch`)
  - Handlers only on the first page; popups / `newPage` omit evidence.
  - Fix: `context.on("page", …)` shared attach helper.

- [ ] **P2** Lighter Chromium launch for ephemeral sessions
  (`apps/browser-service/src/browser.ts` — `launch`)
  - Persistent context + full resource load is heavier than needed for
    headless verification.
  - Fix: prefer `chromium.launch` + context when no profile persistence is
    required; add args (`--disable-dev-shm-usage`, optional image blocking
    via route) for lower RAM/CPU.

- [ ] **P2** Narrow start-menu `setState` rebuilds
  (`apps/desktop/lib/src/desktop_shell.dart` — `_DesktopShellState`)
  - Toggling the start menu rebuilds title bar, menu bar, and workspace.
  - Fix: localize overlay state (or `ValueListenableBuilder`) so chrome stays
    const/stable.

- [ ] **P2** Harden `CoreClient` line handling / sync I/O
  (`apps/desktop/lib/src/core_client.dart`)
  - `jsonDecode` in `_handleLine` can kill the subscription; `existsSync` in
    `_launchCore` blocks the UI isolate.
  - Fix: try/catch per line; `File.exists` async (or resolve executable once).

- [ ] **P2** Skip website asset regeneration when outputs are fresh
  (`apps/website/scripts/generate-brand-assets.mjs`, `package.json` `build`)
  - Sharp regenerates committed PNGs on every `npm run build` / CI test.
  - Fix: mtime/hash short-circuit, or generate only in `npm run assets` and
    verify presence in build.

- [ ] **P2** Add caches to release workflow; pin Bun
  (`.github/workflows/release.yml`, `ci.yml` browser-service)
  - Release lacks `Swatinem/rust-cache` / Flutter `cache: true`; CI uses
    `bun-version: latest` (cache thrash) and no Bun install cache.
  - Fix: mirror CI caches; pin Bun; cache Bun install directory.

- [ ] **P2** Reduce duplicate Rust compile work in CI
  (`.github/workflows/ci.yml` — `rust` job)
  - `clippy --all-targets` then `cargo test` largely recompiles.
  - Fix: `cargo clippy` then `cargo test --all-targets` with shared
    `CARGO_TARGET_DIR` / sccache, or nextest after a single build.

### P3 — polish

- [ ] **P3** `writeLine` can settle the Promise twice when `stdout.write`
  returns true (`apps/browser-service/src/main.ts`)
- [ ] **P3** Cache default `buildLunaDarkTheme()` result
  (`packages/retcon-design-system/lib/src/theme.dart`)
- [ ] **P3** Hoist `Actions` map / avoid realloc on each shell build
  (`apps/desktop/lib/src/desktop_shell.dart`)
- [ ] **P3** Split `astro check` out of default `npm test` hot path
  (`apps/website/package.json`) when iteration speed matters
- [ ] **P3** Expand `audit.yml` path filters to website lockfile + pubspecs
- [ ] **P3** Emit/log stdout event-drop counters when backlog is saturated

### Suggested order of attack (non-Rust)

1. Per-entry + stdin line caps; clear buffers on close (bounds)
2. Fix real stdio concurrency; throttle live network emits (throughput)
3. CI Playwright cache + path filters (developer time)
4. Flutter root ownership + shell rebuild narrowing (desktop footing)
5. Ring buffer / Chromium launch / asset short-circuit (incremental)
