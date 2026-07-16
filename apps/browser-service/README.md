# Retcon browser service

Playwright-driven managed Chromium, isolated from the desktop shell in its own process
(product principle: a browser crash must never take down the UI).

## Phase 1 status

Process skeleton only: structured JSON logging (same record shape as `retcon-core`),
graceful shutdown, and an RPC seam that Phase 4 replaces with the real named-pipe
transport. Chromium lifecycle work is Phase 2 (spike) and Phase 24 (full service).

## Commands

```sh
bun install          # install dependencies
bun run start        # run the service (Ctrl-C to stop)
bun run health       # init, log a health record, exit 0
bun test             # unit tests
bun run typecheck    # tsc --noEmit
bun run lint         # biome check
```

Playwright's Chromium download is deliberately **not** run at install time; managed
installation is a Phase 24 responsibility (`npx playwright install chromium` when needed).
