import { mkdir, realpath, stat } from "node:fs/promises";
import { isAbsolute, relative, resolve, sep } from "node:path";

export const MAX_TEXT_BYTES = 1_048_576;
export const MAX_SCRIPT_BYTES = 65_536;
export const MAX_ARTIFACT_BYTES = 100 * 1024 * 1024;
export const DEFAULT_TIMEOUT_MS = 30_000;

export function objectParam(value: unknown, name = "params"): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${name} must be an object`);
  }
  return value as Record<string, unknown>;
}

export function stringParam(
  value: unknown,
  name: string,
  options: { required?: boolean; max?: number } = {},
): string {
  if (value === undefined && !options.required) return "";
  if (typeof value !== "string" || (options.required && value.trim().length === 0)) {
    throw new Error(`${name} must be a non-empty string`);
  }
  const max = options.max ?? 4_096;
  if (Buffer.byteLength(value) > max) throw new Error(`${name} exceeds ${max} bytes`);
  return value;
}

export function boundedNumber(
  value: unknown,
  name: string,
  minimum: number,
  maximum: number,
  fallback: number,
): number {
  if (value === undefined) return fallback;
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new Error(`${name} must be a finite number`);
  }
  if (value < minimum || value > maximum) {
    throw new Error(`${name} must be between ${minimum} and ${maximum}`);
  }
  return value;
}

export function booleanParam(value: unknown, name: string, fallback = false): boolean {
  if (value === undefined) return fallback;
  if (typeof value !== "boolean") throw new Error(`${name} must be a boolean`);
  return value;
}

export function identifier(value: unknown, name: string, fallback = "default"): string {
  const candidate =
    value === undefined ? fallback : stringParam(value, name, { required: true, max: 64 });
  if (!/^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$/.test(candidate)) {
    throw new Error(`${name} contains unsupported characters`);
  }
  return candidate;
}

export function timeoutParam(value: unknown): number {
  return Math.trunc(boundedNumber(value, "timeoutMs", 100, 120_000, DEFAULT_TIMEOUT_MS));
}

export function boundedJson(value: unknown, maxBytes = MAX_TEXT_BYTES): unknown {
  const encoded = JSON.stringify(value);
  if (encoded === undefined) return null;
  if (Buffer.byteLength(encoded) > maxBytes) throw new Error(`result exceeds ${maxBytes} bytes`);
  return JSON.parse(encoded) as unknown;
}

export function boundedText(
  value: string,
  maxBytes = MAX_TEXT_BYTES,
): {
  text: string;
  truncated: boolean;
} {
  const bytes = Buffer.from(value);
  if (bytes.length <= maxBytes) return { text: value, truncated: false };
  return { text: bytes.subarray(0, maxBytes).toString("utf8"), truncated: true };
}

export function safeUrl(raw: unknown, fileRoots: string[]): URL {
  const value = stringParam(raw, "url", { required: true, max: 16_384 });
  let parsed: URL;
  try {
    parsed = new URL(value);
  } catch {
    throw new Error("url must be an absolute URL");
  }
  if (parsed.username || parsed.password)
    throw new Error("URLs containing credentials are not allowed");
  if (parsed.protocol === "http:" || parsed.protocol === "https:") return parsed;
  if (parsed.protocol !== "file:")
    throw new Error("only HTTP(S) and approved file URLs are allowed");
  const filePath = decodeURIComponent(parsed.pathname).replace(/^\/(?:([A-Za-z]:))/, "$1");
  if (!fileRoots.some((root) => isWithin(root, resolve(filePath)))) {
    throw new Error("file URL is outside approved input roots");
  }
  return parsed;
}

export function isWithin(root: string, candidate: string): boolean {
  const rel = relative(resolve(root), resolve(candidate));
  return rel === "" || (!rel.startsWith(`..${sep}`) && rel !== ".." && !isAbsolute(rel));
}

export async function ensureRoot(root: string): Promise<string> {
  await mkdir(root, { recursive: true });
  return realpath(root);
}

export async function safeOutputPath(
  root: string,
  requested: unknown,
  fallbackName: string,
): Promise<string> {
  const raw =
    requested === undefined || requested === ""
      ? fallbackName
      : stringParam(requested, "path", { required: true, max: 4_096 });
  const candidate = isAbsolute(raw) ? resolve(raw) : resolve(root, raw);
  if (!isWithin(root, candidate))
    throw new Error("output path is outside the managed artifact root");
  const parent = resolve(candidate, "..");
  await mkdir(parent, { recursive: true });
  const realParent = await realpath(parent);
  if (!isWithin(root, realParent))
    throw new Error("output path traverses outside the managed artifact root");
  return candidate;
}

export async function safeInputPath(roots: string[], requested: unknown): Promise<string> {
  const raw = stringParam(requested, "input path", { required: true, max: 4_096 });
  const candidate = await realpath(resolve(raw));
  if (!roots.some((root) => isWithin(root, candidate))) {
    throw new Error("input path is outside approved input roots");
  }
  const metadata = await stat(candidate);
  if (!metadata.isFile()) throw new Error("input path must identify a file");
  return candidate;
}

export async function enforceFileSize(
  path: string,
  maxBytes = MAX_ARTIFACT_BYTES,
): Promise<number> {
  const size = (await stat(path)).size;
  if (size > maxBytes) throw new Error(`artifact exceeds ${maxBytes} bytes`);
  return size;
}

export async function withTimeout<T>(
  operation: Promise<T>,
  timeoutMs: number,
  signal?: AbortSignal,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let abort: (() => void) | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(
      () => reject(new Error(`operation timed out after ${timeoutMs}ms`)),
      timeoutMs,
    );
  });
  const cancelled = new Promise<never>((_, reject) => {
    if (!signal) return;
    abort = () => reject(new Error("operation cancelled"));
    if (signal.aborted) abort();
    else signal.addEventListener("abort", abort, { once: true });
  });
  try {
    return await Promise.race([operation, timeout, cancelled]);
  } finally {
    if (timer) clearTimeout(timer);
    if (signal && abort) signal.removeEventListener("abort", abort);
  }
}
