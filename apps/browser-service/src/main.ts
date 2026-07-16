import { createInterface } from "node:readline";
import { ManagedBrowser, type LogQuery, type ScreenshotOptions } from "./browser";
import { logger } from "./logging";
import { connect } from "./rpc";

const VERSION = "0.1.0";
const MAX_STDOUT_BACKLOG = 64;
/** Align with retcon-protocol MAX_FRAME_BYTES so one stdin line cannot OOM the service. */
const MAX_STDIN_LINE_CHARS = 1024 * 1024;
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
    process.stdout.write(line, (error) => {
      if (error) reject(error);
      else resolve();
    });
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

async function handleRequest(
  browser: ManagedBrowser,
  request: RpcRequest,
): Promise<Record<string, unknown>> {
  const params = request.params ?? {};
  switch (request.method) {
    case "browser.launch":
      return browser.launch();
    case "browser.navigate":
      return browser.navigate(String(params.url ?? ""));
    case "browser.screenshot":
      return browser.screenshot({
        path: String(params.path ?? ""),
        fullPage: Boolean(params.fullPage),
        type: params.type === "jpeg" ? "jpeg" : "png",
        quality: typeof params.quality === "number" ? params.quality : undefined,
      } satisfies ScreenshotOptions);
    case "browser.action":
      return browser.action(params);
    case "browser.logs":
      return browser.logs({
        offset: typeof params.offset === "number" ? params.offset : undefined,
        limit: typeof params.limit === "number" ? params.limit : undefined,
      } satisfies LogQuery);
    case "browser.status":
      return browser.status();
    case "browser.close":
      return browser.close();
    default:
      throw new Error(`unknown method: ${request.method}`);
  }
}

async function serveStdio(browser: ManagedBrowser): Promise<void> {
  const lines = createInterface({ input: process.stdin, crlfDelay: Number.POSITIVE_INFINITY });
  let mutating: Promise<void> | undefined;
  let activeMutating = false;

  for await (const line of lines) {
    if (line.length > MAX_STDIN_LINE_CHARS) {
      await writeLine({ id: 0, error: { message: "oversized request line" } });
      continue;
    }

    let request: RpcRequest;
    try {
      request = JSON.parse(line) as RpcRequest;
    } catch {
      await writeLine({ id: 0, error: { message: "malformed JSON" } });
      continue;
    }

    if (!isMutatingMethod(request.method)) {
      try {
        const result = await handleRequest(browser, request);
        await writeLine({ id: request.id, result });
      } catch (error) {
        await writeLine({
          id: request.id,
          error: { message: error instanceof Error ? error.message : String(error) },
        });
      }
      continue;
    }

    if (activeMutating) {
      await writeLine({
        id: request.id,
        error: { message: "browser service is busy with another mutating request" },
      });
      continue;
    }

    activeMutating = true;
    mutating = (async () => {
      try {
        const result = await handleRequest(browser, request);
        await writeLine({ id: request.id, result });
      } catch (error) {
        await writeLine({
          id: request.id,
          error: { message: error instanceof Error ? error.message : String(error) },
        });
      } finally {
        activeMutating = false;
      }
    })();
  }

  if (mutating) await mutating;
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
