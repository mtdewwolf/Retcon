/**
 * Local transport client for retcon-core using generated protocol v1 types.
 */

import { connect as netConnect, type Socket } from "node:net";
import { createInterface } from "node:readline";

import type {
  ClientHello,
  Discovery,
  PingFrame,
  PongFrame,
  RpcRequest,
  RpcResponse,
  ServerHello,
} from "./generated/protocol-v1";
import { PROTOCOL_VERSION } from "./generated/protocol-v1";
import { logger } from "./logging";

const CLIENT_VERSION = "0.1.0";
const CLIENT_FEATURES = ["ping", "events.replay", "request.cancel"];

export interface RpcClient {
  readonly connected: boolean;
  request(
    method: string,
    params?: Record<string, unknown>,
  ): Promise<Record<string, unknown>>;
  ping(): Promise<void>;
  close(): Promise<void>;
}

class CoreRpcClient implements RpcClient {
  private readonly socket: Socket;
  private readonly pending = new Map<
    number,
    {
      resolve: (value: Record<string, unknown>) => void;
      reject: (error: Error) => void;
    }
  >();
  private nextId = 0;
  private closed = false;
  private readonly lines: AsyncIterable<string>;

  private constructor(socket: Socket, lines: AsyncIterable<string>) {
    this.socket = socket;
    this.lines = lines;
    void this.readLoop();
  }

  get connected(): boolean {
    return !this.closed && !this.socket.destroyed;
  }

  static async connect(discovery: Discovery): Promise<CoreRpcClient> {
    const socket = await openTransport(discovery);
    const lines = createInterface({
      input: socket,
      crlfDelay: Number.POSITIVE_INFINITY,
    });
    await writeLine(socket, { auth: discovery.token });
    const hello: ClientHello = {
      kind: "client.hello",
      protocolVersion: PROTOCOL_VERSION,
      clientVersion: CLIENT_VERSION,
      features: CLIENT_FEATURES,
    };
    await writeLine(socket, hello);
    const serverHello = await readJsonLine<ServerHello>(lines);
    if (serverHello.kind !== "server.hello") {
      throw new Error("expected server.hello during handshake");
    }
    logger.info("connected to retcon-core", {
      serverVersion: serverHello.serverVersion,
      features: serverHello.features,
    });
    return new CoreRpcClient(socket, lines);
  }

  async request(
    method: string,
    params: Record<string, unknown> = {},
  ): Promise<Record<string, unknown>> {
    if (!this.connected) throw new Error("RPC transport is disconnected");
    const id = ++this.nextId;
    const frame: RpcRequest = { id, method, params };
    const result = new Promise<Record<string, unknown>>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
    });
    await writeLine(this.socket, frame);
    return result;
  }

  async ping(): Promise<void> {
    const frame: PingFrame = { kind: "ping" };
    await writeLine(this.socket, frame);
  }

  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    for (const pending of this.pending.values()) {
      pending.reject(new Error("RPC transport closed"));
    }
    this.pending.clear();
    await new Promise<void>((resolve) => {
      this.socket.end(() => resolve());
    });
  }

  private async readLoop(): Promise<void> {
    try {
      for await (const line of this.lines) {
        const frame = JSON.parse(line) as RpcResponse | PongFrame | { event: unknown };
        if ("kind" in frame && frame.kind === "pong") continue;
        if ("event" in frame) continue;
        const response = frame as RpcResponse;
        const pending = this.pending.get(response.id);
        if (!pending) continue;
        this.pending.delete(response.id);
        if (response.error) {
          pending.reject(new Error(response.error.user_message));
        } else {
          pending.resolve(response.result ?? {});
        }
      }
    } catch (error) {
      logger.warn("RPC read loop ended", {
        error: error instanceof Error ? error.message : String(error),
      });
    } finally {
      await this.close();
    }
  }
}

function openTransport(discovery: Discovery): Promise<Socket> {
  return new Promise((resolve, reject) => {
    const socket =
      discovery.transport === "unix_socket"
        ? netConnect({ path: discovery.path })
        : netConnect(discovery.path);
    socket.once("connect", () => resolve(socket));
    socket.once("error", reject);
  });
}

async function writeLine(socket: Socket, payload: unknown): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    socket.write(`${JSON.stringify(payload)}\n`, (error) => {
      if (error) reject(error);
      else resolve();
    });
  });
}

async function readJsonLine<T>(lines: AsyncIterable<string>): Promise<T> {
  for await (const line of lines) {
    return JSON.parse(line) as T;
  }
  throw new Error("transport closed before handshake completed");
}

/**
 * Connect to retcon-core using a discovery pipe path or JSON discovery file path.
 */
export function connect(pipeName: string | undefined): RpcClient {
  if (!pipeName) {
    logger.info("rpc transport not configured; running standalone");
    return new DisconnectedRpcClient();
  }
  void CoreRpcClient.connect({
    transport: pipeName.includes("pipe\\") ? "named_pipe" : "unix_socket",
    path: pipeName,
    token: "",
    pid: 0,
    version: CLIENT_VERSION,
    protocolVersion: PROTOCOL_VERSION,
  }).catch((error: unknown) => {
    logger.warn("rpc transport connection failed", {
      error: error instanceof Error ? error.message : String(error),
    });
  });
  return new DisconnectedRpcClient();
}

/**
 * Connect using a parsed discovery record (token + transport path).
 */
export async function connectDiscovery(discovery: Discovery): Promise<RpcClient> {
  return CoreRpcClient.connect(discovery);
}

class DisconnectedRpcClient implements RpcClient {
  readonly connected = false;
  async request(): Promise<Record<string, unknown>> {
    throw new Error("RPC transport is disconnected");
  }
  async ping(): Promise<void> {}
  async close(): Promise<void> {}
}
