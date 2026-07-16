/**
 * Structured logging for the browser service.
 *
 * Emits newline-delimited JSON to stderr, matching the shape used by
 * retcon-core so support bundles can interleave logs from all components.
 */

export type LogLevel = "debug" | "info" | "warn" | "error";

const LEVEL_ORDER: Record<LogLevel, number> = { debug: 10, info: 20, warn: 30, error: 40 };

const ACTIVE_LEVEL: LogLevel = (() => {
  const raw = (process.env.RETCON_LOG ?? "info").toLowerCase();
  return raw in LEVEL_ORDER ? (raw as LogLevel) : "info";
})();

export function log(level: LogLevel, message: string, fields: Record<string, unknown> = {}): void {
  if (LEVEL_ORDER[level] < LEVEL_ORDER[ACTIVE_LEVEL]) return;
  const record = {
    timestamp: new Date().toISOString(),
    level: level.toUpperCase(),
    target: "retcon_browser_service",
    fields: { message, ...fields },
  };
  process.stderr.write(`${JSON.stringify(record)}\n`);
}

export const logger = {
  debug: (message: string, fields?: Record<string, unknown>) => log("debug", message, fields),
  info: (message: string, fields?: Record<string, unknown>) => log("info", message, fields),
  warn: (message: string, fields?: Record<string, unknown>) => log("warn", message, fields),
  error: (message: string, fields?: Record<string, unknown>) => log("error", message, fields),
};
