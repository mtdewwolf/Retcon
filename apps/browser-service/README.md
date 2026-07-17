# Retcon browser service

Managed Playwright Chromium runs outside the desktop shell so a browser crash cannot take
down the UI. Core starts this package over newline-delimited stdio and forwards calls through
`browser.call`.

## Setup and verification

```sh
bun install
bun run browser:install  # installs the Chromium revision for the pinned Playwright version
bun run browser:verify   # reports a real Chromium launch/title/close
bun test                 # unit tests (real-browser test is gated)
bun run test:e2e         # real Chromium against a local fixture
bun run typecheck
bun run lint
```

On Windows, Bun can stall in Playwright's browser transport. `bun run start` therefore
re-executes the TypeScript entrypoint with Node 24 while preserving stdin/stdout/stderr. The
Core launch command remains backward compatible and Bun continues to own install, test, lint,
and typecheck commands.

## RPC surface

Every request is `{ "id": number, "method": "browser.*", "params": object }`. Mutations are
serialized, read-only status/log calls can run concurrently, `browser.cancel` accepts a
`requestId`, and all operations have bounded timeouts.

When Core sets `RETCON_BROWSER_AUTH_TOKEN`, stdio starts locked. Core must first send
`service.hello` with the token and protocol version; incompatible or unauthenticated clients are
rejected before browser state can be touched. Core configures managed artifact/profile roots and
may approve canonical project input roots after the handshake. Development discovery uses
`apps/browser-service`; packaged layouts can set `RETCON_BROWSER_SERVICE_DIR` and
`RETCON_NODE_PATH` explicitly.

- Lifecycle: `installation`, `launch`, `status`, `close`, `kill`, `recover`
- Sessions and tabs: `tab.new`, `tab.list`, `tab.select`, `tab.close`
- Navigation: `navigate`, `back`, `forward`, `reload`, `stop`
- Configuration: `configure`, `viewport.set`, `cookies.get|set|clear`, `storage.get|set`
- Evidence: `screenshot`, `snapshot`, `performance`, `logs`, `trace.start|stop`
- Actions: `click`, `fill`, `type`, `select`, `upload`, `key`, `scroll`, `drag`, `wait`,
  `script`, `text`, `download`, `dialog`, and `popup`; these work as dedicated
  `browser.<action>` methods or through `browser.action` with an `action` parameter.
- Takeover: `pause`, `resume`, `takeover.open`, `takeover.resume`

Temporary profiles are isolated under the OS temporary directory and always removed on close
or crash. Persistent profiles are identifier-based children of `output/browser-profiles` and
can only be deleted explicitly. Screenshots, downloads, traces, and video are restricted to
`output/playwright`; uploads and `file:` URLs must resolve inside approved input roots. Text,
JSON, artifact sizes, cookies, requests, and console/network/page-error logs are bounded.

## Current limitations

- Headed takeover preserves the managed profile and URL by closing and relaunching the context;
  it does not attach to an arbitrary user-owned Chrome process. Automation is rejected for the
  full audited takeover interval.
- Video must be enabled at launch with `recordVideo: true`; Playwright finalizes video paths when
  the session closes. Trace capture supports one active trace per session.
- Cancellation closes the active tab to interrupt Playwright work, then creates a clean tab in
  the same context. DOM state in the cancelled tab is intentionally discarded, while profile,
  cookies, and other context state remain.
- Browser binaries are machine-level Playwright assets and are not committed to the repository.
  `RETCON_CHROMIUM_PATH` can select an approved externally managed Chromium executable.
