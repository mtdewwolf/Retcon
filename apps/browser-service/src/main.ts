import { createInterface } from "node:readline";
import { type LogQuery, ManagedBrowser, type ScreenshotOptions } from "./browser";
import { logger } from "./logging";
import { connect } from "./rpc";

const VERSION = "0.1.0";
const MAX_STDOUT_BACKLOG = 64;
type RpcRequest = { id: number; method: string; params?: Record<string, unknown> };

function parseArgs(argv: string[]): { health: boolean; stdio: boolean; pipe: string | undefined } {
  const pipeIndex = argv.indexOf("--pipe");
  return {
    health: argv.includes("--health"),
    stdio: argv.includes("--stdio"),
    pipe: pipeIndex >= 0 ? argv[pipeIndex + 1] : undefined,
  };
}

function writeLine(payload: unknown): Promise<void> {
  return new Promise((resolve, reject) => {
    const line = `${JSON.stringify(payload)}\n`;
    const accepted = process.stdout.write(line, (error) => {
      if (error) reject(error);
      else resolve();
    });
    if (accepted) resolve();
  });
}

async function emitEvent(
  backlog: { count: number },
  event: { type: string; payload: Record<string, unknown> },
): Promise<void> {
  if (backlog.count >= MAX_STDOUT_BACKLOG) return;
  backlog.count += 1;
  try {
    await writeLine({ event });
  } finally {
    backlog.count -= 1;
  }
}

function isMutatingMethod(method: string): boolean {
  return method !== "browser.status" && method !== "browser.logs";
}

async function serveStdio(browser: ManagedBrowser): Promise<void> {
  const lines = createInterface({ input: process.stdin, crlfDelay: Number.POSITIVE_INFINITY });
  const backlog = { count: 0 };
  let activeMutating = false;
  for await (const line of lines) {
    let request: RpcRequest;
    try {
      request = JSON.parse(line) as RpcRequest;
    } catch {
      await writeLine({ id: 0, error: { message: "malformed JSON" } });
      continue;
    }
    if (isMutatingMethod(request.method) && activeMutating) {
      await writeLine({
        id: request.id,
        error: { message: "browser service is busy with another mutating request" },
      });
      continue;
    }
    if (isMutatingMethod(request.method)) activeMutating = true;
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
        case "browser.screenshot": {
          const screenshotOpts: ScreenshotOptions = {
            path: String(params.path ?? ""),
            fullPage: Boolean(params.fullPage),
            type: params.type === "jpeg" ? "jpeg" : "png",
          };
          if (typeof params.quality === "number") {
            screenshotOpts.quality = params.quality;
          }
          result = await browser.screenshot(screenshotOpts);
          break;
        }
        case "browser.action":
          result = await browser.action(params);
          break;
        case "browser.logs": {
          const logQuery: LogQuery = {};
          if (typeof params.offset === "number") {
            logQuery.offset = params.offset;
          }
          if (typeof params.limit === "number") {
            logQuery.limit = params.limit;
          }
          result = browser.logs(logQuery);
          break;
        }
        case "browser.status":
          result = browser.status();
          break;
        case "browser.close":
          result = await browser.close();
          break;
        default:
          throw new Error(`unknown method: ${request.method}`);
      }
      await writeLine({ id: request.id, result });
    } catch (error) {
      await writeLine({
        id: request.id,
        error: { message: error instanceof Error ? error.message : String(error) },
      });
    } finally {
      if (isMutatingMethod(request.method)) activeMutating = false;
    }
  }
}

async function main(): Promise<number> {
  const args = parseArgs(process.argv.slice(2));
  logger.info("browser service starting", { version: VERSION, pid: process.pid });
  if (args.health) return 0;
  const backlog = { count: 0 };
  const emit = (event: { type: string; payload: Record<string, unknown> }): void => {
    void emitEvent(backlog, event);
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
