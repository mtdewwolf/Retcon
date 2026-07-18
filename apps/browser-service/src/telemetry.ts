/**
 * Content-free runtime instrumentation for the isolated browser process.
 *
 * Records are local newline-delimited JSON on stderr. The vocabulary is closed
 * intentionally: callers cannot attach URLs, selectors, paths, credentials, or
 * other request content.
 */

export const RUNTIME_METRIC_NAMES = [
  "browser.service.lifecycle.count",
  "browser.operation.duration",
  "browser.verification.duration",
  "browser.failure.count",
] as const;

export type RuntimeMetricName = (typeof RUNTIME_METRIC_NAMES)[number];
export type RuntimeOutcome = "cancelled" | "error" | "ok" | "timeout";
export type RuntimeOperation =
  | "action"
  | "close"
  | "launch"
  | "navigation"
  | "observation"
  | "service_crash"
  | "service_start"
  | "service_stop"
  | "session"
  | "takeover"
  | "trace"
  | "verification";

export interface RuntimeMetric {
  name: RuntimeMetricName;
  operation: RuntimeOperation;
  outcome: RuntimeOutcome;
  value: number;
  unit: "count" | "milliseconds";
}

export interface RuntimeRecorder {
  recordMetric(metric: RuntimeMetric): void;
}

const METRIC_NAMES = new Set<string>(RUNTIME_METRIC_NAMES);
const OPERATIONS = new Set<string>([
  "action",
  "close",
  "launch",
  "navigation",
  "observation",
  "service_crash",
  "service_start",
  "service_stop",
  "session",
  "takeover",
  "trace",
  "verification",
]);
const OUTCOMES = new Set<string>(["cancelled", "error", "ok", "timeout"]);
const MAX_DURATION_MS = 24 * 60 * 60 * 1_000;
const MAX_COUNT = 1_000_000;

export class NoopRuntimeRecorder implements RuntimeRecorder {
  recordMetric(_metric: RuntimeMetric): void {}
}

export class StructuredRuntimeRecorder implements RuntimeRecorder {
  private enabled: boolean;
  private readonly write: (line: string) => void;

  constructor(options: { enabled?: boolean; write?: (line: string) => void } = {}) {
    const configured = process.env.RETCON_OBSERVABILITY?.toLowerCase();
    this.enabled = options.enabled ?? (configured === "1" || configured === "on");
    this.write = options.write ?? ((line) => process.stderr.write(line));
  }

  recordMetric(metric: RuntimeMetric): void {
    if (!this.enabled) return;
    if (
      !METRIC_NAMES.has(metric.name) ||
      !OPERATIONS.has(metric.operation) ||
      !OUTCOMES.has(metric.outcome) ||
      !Number.isFinite(metric.value)
    ) {
      return;
    }
    const maximum = metric.unit === "milliseconds" ? MAX_DURATION_MS : MAX_COUNT;
    const value = Math.min(maximum, Math.max(0, metric.value));
    const record = {
      timestamp: new Date().toISOString(),
      level: "INFO",
      target: "retcon_browser_service",
      fields: {
        message: "runtime metric",
        event: "runtime.metric",
        component: "browser_service",
        metric: metric.name,
        operation: metric.operation,
        outcome: metric.outcome,
        unit: metric.unit,
        value,
      },
    };
    try {
      this.write(`${JSON.stringify(record)}\n`);
    } catch {
      // Instrumentation must never affect the browser operation it observes.
    }
  }

  setEnabled(enabled: boolean): void {
    this.enabled = enabled;
  }
}

export function browserOperation(method: string): RuntimeOperation {
  if (method === "browser.launch") return "launch";
  if (method === "browser.close") return "close";
  if (method === "browser.navigate") return "navigation";
  if (method === "browser.action" || method.startsWith("browser.mouse.")) return "action";
  if (method.startsWith("browser.verification.")) return "verification";
  if (method.startsWith("browser.trace.")) return "trace";
  if (method.startsWith("browser.takeover.")) return "takeover";
  if (method.startsWith("browser.session.") || method.startsWith("browser.tab.")) return "session";
  return "observation";
}

export function outcomeFor(error: unknown): RuntimeOutcome {
  const message = error instanceof Error ? error.message : String(error);
  if (message.includes("cancelled") || message.includes("aborted")) return "cancelled";
  if (message.includes("timed out") || message.includes("timeout")) return "timeout";
  return "error";
}

export function recordDuration(
  recorder: RuntimeRecorder,
  name: RuntimeMetricName,
  operation: RuntimeOperation,
  startedAt: number,
  outcome: RuntimeOutcome,
): void {
  recorder.recordMetric({
    name,
    operation,
    outcome,
    unit: "milliseconds",
    value: performance.now() - startedAt,
  });
}
