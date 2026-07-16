import { ManagedBrowser, type LogQuery, type ScreenshotOptions } from "./browser";
import { logger } from "./logging";
import { connect } from "./rpc";

const VERSION = "0.1.0";
const MAX_STDOUT_BACKLOG = 64;
/** Match core RPC frame budget so a hostile client cannot OOM via stdin. */
export const MAX_RPC_LINE_BYTES = 1024 * 1024;
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

/** Read newline-delimited stdin frames with a hard byte cap. */
export async function* readCappedLines(
  input: NodeJS.ReadableStream,
  maxBytes = MAX_RPC_LINE_BYTES,
): AsyncGenerator<string> {
  let buffer = Buffer.alloc(0);
  for await (const chunk of input) {
    const piece = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    if (buffer.length + piece.length > maxBytes + 1) {
      throw new Error(`RPC line exceeds ${maxBytes} bytes`);
    }
    buffer = Buffer.concat([buffer, piece]);
    let newline = buffer.indexOf(0x0a);
    while (newline >= 0) {
      const line = buffer.subarray(0, newline).toString("utf8").replace(/\r$/, "");
      buffer = buffer.subarray(newline + 1);
      if (line.length > 0) yield line;
      newline = buffer.indexOf(0x0a);
    }
    if (buffer.length > maxBytes) {
      throw new Error(`RPC line exceeds ${maxBytes} bytes`);
    }
  }
  if (buffer.length > 0) {
    yield buffer.toString("utf8").replace(/\r$/, "");
  }
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
  const backlog = { count: 0 };
  let activeMutating: Promise<void> | undefined;
  let mutatingChain: Promise<void> = Promise.resolve();

  for await (const line of readCappedLines(process.stdin)) {
    let request: RpcRequest;
    try {
      request = JSON.parse(line) as RpcRequest;
    } catch {
      await writeLine({ id: 0, error: { message: "malformed JSON" } });
      continue;
    }

    const respond = async (): Promise<void> => {
      try {
        const result = await handleRequest(browser, request);
        await writeLine({ id: request.id, result });
      } catch (error) {
        await writeLine({
          id: request.id,
          error: { message: error instanceof Error ? error.message : String(error) },
        });
      }
    };

    if (isMutatingMethod(request.method)) {
      // Serialize mutating methods; do not block the stdin read loop.
      const run = mutatingChain.then(async () => {
        activeMutating = respond();
        try {
          await activeMutating;
        } finally {
          activeMutating = undefined;
        }
      });
      mutatingChain = run.catch(() => undefined);
      void run;
    } else {
      // Status/logs may overlap an in-flight mutating request.
      void respond();
    }
  }

  await mutatingChain;
  if (activeMutating) await activeMutating;
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
  if (args.stdio) {
    try {
      await serveStdio(browser);
    } catch (error) {
      logger.error("stdio RPC failed", {
        error: error instanceof Error ? error.message : String(error),
      });
      await shutdown("stdio-error");
      return 1;
    }
  } else await new Promise(() => undefined);
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
