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
bun run test:verification-e2e # verification, visual diff, responsive, and a11y fixture
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
- Verification: `verification.run`

## Browser verification contract

Authenticated stdio clients run a self-contained definition with
`browser.verification.run`. The request is bounded to 128 ordered steps, 12 responsive
variants, three retries, and a 120-second definition timeout:

```json
{
  "runId": "checkout-42",
  "artifactDirectory": "verifications/checkout-42",
  "timeoutMs": 120000,
  "definition": {
    "id": "checkout",
    "targetUrl": "http://127.0.0.1:3000",
    "serverRef": "dev-server-42",
    "timeoutMs": 30000,
    "retries": 1,
    "failOnAccessibility": true,
    "variants": [{ "name": "desktop", "width": 1280, "height": 720 }],
    "visual": {
      "updateBaseline": "missing",
      "pixelThreshold": 0.1,
      "maxDiffPixelRatio": 0.001,
      "perceptualThreshold": 0.01,
      "maskSelectors": ["[data-dynamic]"],
      "ignoreSelectors": []
    },
    "steps": [
      { "id": "load", "type": "navigate", "expectedStatus": 200 },
      { "id": "name", "type": "fill", "selector": "#name", "value": "Retcon" },
      { "id": "submit", "type": "click", "selector": "button" },
      { "id": "proof", "type": "assert.text", "selector": "main", "expected": "Saved" },
      { "id": "evidence", "type": "screenshot", "name": "checkout", "fullPage": true }
    ]
  }
}
```

Steps support navigation, click/fill/press/wait interactions, text/element/status/console
assertions, screenshots, and explicit accessibility gates. Every run also performs an
accessibility audit covering labels, contrast, keyboard focusability and a bounded Tab-trap
heuristic, heading order, landmarks, forms, and visible focus indicators.

The deterministic response contains `status`, ordered `timeline`, `assertions`, responsive
variant results, `visualComparisons`, accessibility findings, console/network/page-error
summaries, and managed artifact paths. Visual `diffPixelRatio`, `maxDiffPixelRatio`, and
`perceptualDifference` values are fractions from 0 to 1. Baseline updates preserve the prior
20 versions; current, baseline, and diff PNGs are reviewable and capped at 16 MiB each. No
image bytes, request headers, cookies, or form values are returned over stdio. Sensitive text
and URL query values are redacted. `browser.cancel` cancels a running verification through the
existing request abort controller.

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
- The built-in accessibility audit is a deterministic completion gate, not a replacement for
  assistive-technology testing. Its keyboard-trap check is intentionally a bounded heuristic.
