import { createInterface } from "node:readline";
import { ManagedBrowser } from "./browser";
import { logger } from "./logging";
import { connect } from "./rpc";

const VERSION = "0.1.0";
type RpcRequest = { id: number; method: string; params?: Record<string, unknown> };

function parseArgs(argv: string[]): { health: boolean; stdio: boolean; pipe: string | undefined } {
  const pipeIndex = argv.indexOf("--pipe");
  return {
    health: argv.includes("--health"),
    stdio: argv.includes("--stdio"),
    pipe: pipeIndex >= 0 ? argv[pipeIndex + 1] : undefined,
  };
}

async function serveStdio(browser: ManagedBrowser): Promise<void> {
  const lines = createInterface({ input: process.stdin, crlfDelay: Number.POSITIVE_INFINITY });
  for await (const line of lines) {
    let request: RpcRequest;
    try {
      request = JSON.parse(line) as RpcRequest;
    } catch {
      process.stdout.write(`${JSON.stringify({ id: 0, error: { message: "malformed JSON" } })}\n`);
      continue;
    }
    try {
      const params = request.params ?? {};
      let result: Record<string, unknown>;
      switch (request.method) {
        case "browser.launch":
          result = await browser.launch();
          break;
        case "browser.navigate":
          result = await browser.navigate(String(params.url ?? ""));
          break;
        case "browser.screenshot":
          result = await browser.screenshot(String(params.path ?? ""));
          break;
        case "browser.action":
          result = await browser.action(params);
          break;
        case "browser.logs":
          result = browser.logs();
          break;
        case "browser.status":
          result = browser.status();
          break;
        case "browser.close":
          result = await browser.close();
          break;
        default:
          throw new Error(`unknown method: ${request.method}`);
      }
      process.stdout.write(`${JSON.stringify({ id: request.id, result })}\n`);
    } catch (error) {
      process.stdout.write(
        `${JSON.stringify({ id: request.id, error: { message: error instanceof Error ? error.message : String(error) } })}\n`,
      );
    }
  }
}

async function main(): Promise<number> {
  const args = parseArgs(process.argv.slice(2));
  logger.info("browser service starting", { version: VERSION, pid: process.pid });
  if (args.health) return 0;
  const emit = (event: { type: string; payload: Record<string, unknown> }): void => {
    process.stdout.write(`${JSON.stringify({ event })}\n`);
  };
  const browser = new ManagedBrowser(emit);
  const rpc = args.stdio ? undefined : connect(args.pipe);
  let shuttingDown = false;
  const shutdown = async (signal: string): Promise<void> => {
    if (shuttingDown) return;
    shuttingDown = true;
    logger.info("browser service stopping", { signal });
    await browser.close();
    await rpc?.close();
  };
  process.on("SIGINT", () => void shutdown("SIGINT").then(() => process.exit(0)));
  process.on("SIGTERM", () => void shutdown("SIGTERM").then(() => process.exit(0)));
  if (args.stdio) await serveStdio(browser);
  else await new Promise(() => undefined);
  await shutdown("stdin-closed");
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
