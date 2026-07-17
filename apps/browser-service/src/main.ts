import { timingSafeEqual } from "node:crypto";
import { delimiter } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { ManagedBrowser } from "./browser.ts";
import { logger } from "./logging.ts";
import { connect } from "./rpc.ts";

const VERSION = "0.1.0";
const MAX_STDOUT_BACKLOG = 64;
const MAX_REQUEST_BYTES = 1_048_576;
const MAX_ACTIVE_REQUESTS = 64;
const PROTOCOL_VERSION = 1;
const FEATURES = [
  "sessions",
  "tabs",
  "navigation",
  "observations",
  "actions",
  "trace",
  "video",
  "takeover",
  "cancel",
  "verification",
];
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

function isReadOnlyMethod(method: string): boolean {
  return ["browser.status", "browser.logs", "browser.installation"].includes(method);
}

function validateRequest(value: unknown): RpcRequest {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error("request must be an object");
  const request = value as Record<string, unknown>;
  if (!Number.isSafeInteger(request.id) || (request.id as number) < 0) {
    throw new Error("request id must be a non-negative safe integer");
  }
  if (
    typeof request.method !== "string" ||
    !/^(?:browser|service)\.[A-Za-z0-9_.-]+$/.test(request.method)
  ) {
    throw new Error("request method must be a browser.* or service.* method");
  }
  if (
    request.params !== undefined &&
    (!request.params || typeof request.params !== "object" || Array.isArray(request.params))
  ) {
    throw new Error("request params must be an object");
  }
  return {
    id: request.id as number,
    method: request.method,
    ...(request.params ? { params: request.params as Record<string, unknown> } : {}),
  };
}

export async function serveStdio(browser: ManagedBrowser): Promise<void> {
  const lines = createInterface({ input: process.stdin, crlfDelay: Number.POSITIVE_INFINITY });
  const active = new Map<number, AbortController>();
  const pending = new Set<Promise<void>>();
  const requiredToken = process.env.RETCON_BROWSER_AUTH_TOKEN;
  let authenticated = !requiredToken;
  let mutationTail = Promise.resolve();
  for await (const line of lines) {
    if (Buffer.byteLength(line) > MAX_REQUEST_BYTES) {
      await writeLine({ id: 0, error: { message: `request exceeds ${MAX_REQUEST_BYTES} bytes` } });
      continue;
    }
    let request: RpcRequest;
    try {
      request = validateRequest(JSON.parse(line) as unknown);
    } catch (error) {
      await writeLine({
        id: 0,
        error: { message: error instanceof Error ? error.message : "malformed JSON" },
      });
      continue;
    }
    if (request.method === "service.hello") {
      const token = typeof request.params?.token === "string" ? request.params.token : "";
      const suppliedProtocol = request.params?.protocolVersion;
      const tokenMatches =
        !requiredToken ||
        (token.length === requiredToken.length &&
          timingSafeEqual(Buffer.from(token), Buffer.from(requiredToken)));
      if (!tokenMatches) {
        await writeLine({
          id: request.id,
          error: { code: "unauthorized", message: "invalid service token" },
        });
        continue;
      }
      if (suppliedProtocol !== PROTOCOL_VERSION) {
        await writeLine({
          id: request.id,
          error: {
            code: "protocol_mismatch",
            message: `browser service protocol ${PROTOCOL_VERSION} does not match ${String(suppliedProtocol)}`,
          },
        });
        continue;
      }
      authenticated = true;
      await writeLine({
        id: request.id,
        result: {
          serviceVersion: VERSION,
          protocolVersion: PROTOCOL_VERSION,
          features: FEATURES,
          healthy: true,
          pid: process.pid,
        },
      });
      continue;
    }
    if (!authenticated) {
      await writeLine({
        id: request.id,
        error: { code: "unauthorized", message: "service handshake required" },
      });
      continue;
    }
    if (request.method === "service.roots.add") {
      try {
        await browser.addInputRoots(request.params?.roots);
        await writeLine({ id: request.id, result: { approved: true } });
      } catch (error) {
        await writeLine({
          id: request.id,
          error: { message: error instanceof Error ? error.message : String(error) },
        });
      }
      continue;
    }
    if (request.method === "service.shutdown") {
      await browser.close();
      await writeLine({ id: request.id, result: { stopped: true } });
      break;
    }
    if (request.method === "browser.cancel") {
      const target = request.params?.requestId;
      if (!Number.isSafeInteger(target)) {
        await writeLine({ id: request.id, error: { message: "requestId must be a safe integer" } });
        continue;
      }
      const controller = active.get(target as number);
      controller?.abort();
      await writeLine({
        id: request.id,
        result: { cancelled: controller !== undefined, requestId: target },
      });
      continue;
    }
    if (active.size >= MAX_ACTIVE_REQUESTS || active.has(request.id)) {
      await writeLine({
        id: request.id,
        error: {
          message: active.has(request.id)
            ? "request id is already active"
            : "too many active requests",
        },
      });
      continue;
    }
    const controller = new AbortController();
    active.set(request.id, controller);
    const execute = async (): Promise<void> => {
      try {
        const result = await browser.call(request.method, request.params ?? {}, controller.signal);
        await writeLine({ id: request.id, result });
      } catch (error) {
        await writeLine({
          id: request.id,
          error: { message: error instanceof Error ? error.message : String(error) },
        });
      } finally {
        active.delete(request.id);
      }
    };
    let task: Promise<void>;
    if (isReadOnlyMethod(request.method)) task = execute();
    else {
      mutationTail = mutationTail.then(execute, execute);
      task = mutationTail;
    }
    pending.add(task);
    void task.finally(() => pending.delete(task));
  }
  lines.close();
  for (const controller of active.values()) controller.abort();
  await Promise.allSettled([...pending]);
}

async function main(): Promise<number> {
  const args = parseArgs(process.argv.slice(2));
  if (typeof Bun !== "undefined" && process.env.RETCON_BROWSER_NODE_CHILD !== "1") {
    const child = Bun.spawn(["node", fileURLToPath(import.meta.url), ...process.argv.slice(2)], {
      stdin: "inherit",
      stdout: "inherit",
      stderr: "inherit",
      env: { ...process.env, RETCON_BROWSER_NODE_CHILD: "1" },
    });
    return child.exited;
  }
  logger.info("browser service starting", { version: VERSION, pid: process.pid });
  if (args.health) return 0;
  const backlog = { count: 0 };
  const emit = (event: { type: string; payload: Record<string, unknown> }): void => {
    void emitEvent(backlog, event);
  };
  const configuredInputRoots = process.env.RETCON_BROWSER_INPUT_ROOTS?.split(delimiter).filter(
    (root) => root.length > 0,
  );
  const browser = new ManagedBrowser(emit, {
    ...(process.env.RETCON_BROWSER_ARTIFACT_ROOT
      ? { artifactRoot: process.env.RETCON_BROWSER_ARTIFACT_ROOT }
      : {}),
    ...(process.env.RETCON_BROWSER_PROFILE_ROOT
      ? { profileRoot: process.env.RETCON_BROWSER_PROFILE_ROOT }
      : {}),
    ...(configuredInputRoots?.length ? { inputRoots: configuredInputRoots } : {}),
  });
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
