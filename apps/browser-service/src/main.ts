/**
 * Entry point for the Retcon browser service.
 *
 * Phase 1 scope: process skeleton with structured logging, an RPC seam, and
 * graceful shutdown. Chromium lifecycle management (launch, isolated profiles,
 * screenshots, crash recovery) lands in Phases 2 and 24.
 */

import { logger } from "./logging";
import { connect } from "./rpc";

const VERSION = "0.1.0";

function parseArgs(argv: string[]): { health: boolean; pipe: string | undefined } {
  const health = argv.includes("--health");
  const pipeIndex = argv.indexOf("--pipe");
  const pipe = pipeIndex >= 0 ? argv[pipeIndex + 1] : undefined;
  return { health, pipe };
}

async function main(): Promise<number> {
  const { health, pipe } = parseArgs(process.argv.slice(2));

  logger.info("browser service starting", { version: VERSION, pid: process.pid });

  if (health) {
    logger.info("health check", { version: VERSION, status: "ok" });
    return 0;
  }

  const rpc = connect(pipe);

  let shuttingDown = false;
  const shutdown = async (signal: string): Promise<void> => {
    if (shuttingDown) return;
    shuttingDown = true;
    logger.info("shutdown signal received; browser service stopping", { signal });
    await rpc.close();
    process.exit(0);
  };
  process.on("SIGINT", () => void shutdown("SIGINT"));
  process.on("SIGTERM", () => void shutdown("SIGTERM"));

  // Keep the process alive until a shutdown signal arrives. Real work
  // (serving browser sessions) replaces this in Phase 2.
  await new Promise(() => {
    /* run until signalled */
  });
  return 0;
}

main().then(
  (code) => {
    if (code !== 0) process.exit(code);
  },
  (error: unknown) => {
    logger.error("browser service crashed", {
      error: error instanceof Error ? error.message : String(error),
    });
    process.exit(1);
  },
);
