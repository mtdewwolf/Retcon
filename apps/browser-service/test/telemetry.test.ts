import { describe, expect, test } from "bun:test";

import {
  NoopRuntimeRecorder,
  RUNTIME_METRIC_NAMES,
  StructuredRuntimeRecorder,
  browserOperation,
  outcomeFor,
} from "../src/telemetry.ts";

describe("content-free runtime telemetry", () => {
  test("uses a stable metric and dimension vocabulary", () => {
    expect(RUNTIME_METRIC_NAMES).toEqual([
      "browser.service.lifecycle.count",
      "browser.operation.duration",
      "browser.verification.duration",
      "browser.failure.count",
    ]);
    expect(browserOperation("browser.navigate")).toBe("navigation");
    expect(browserOperation("browser.verification.run")).toBe("verification");
    expect(browserOperation("browser.dom")).toBe("observation");
    expect(outcomeFor(new Error("operation timed out"))).toBe("timeout");
    expect(outcomeFor(new Error("operation cancelled"))).toBe("cancelled");
    expect(outcomeFor(new Error("credential=https://user:pass@example.invalid"))).toBe("error");
  });

  test("emits only allowlisted fields and bounds values", () => {
    const lines: string[] = [];
    const recorder = new StructuredRuntimeRecorder({
      enabled: true,
      write: (line) => lines.push(line),
    });
    recorder.recordMetric({
      name: "browser.operation.duration",
      operation: "navigation",
      outcome: "ok",
      unit: "milliseconds",
      value: Number.MAX_VALUE,
    });
    const parsed = JSON.parse(lines[0] ?? "{}") as Record<string, unknown>;
    const serialized = JSON.stringify(parsed);
    expect(serialized).not.toContain("example.invalid");
    expect(serialized).not.toContain("password");
    expect(Object.keys(parsed)).toEqual(["timestamp", "level", "target", "fields"]);
    expect(Object.keys(parsed.fields as Record<string, unknown>)).toEqual([
      "message",
      "event",
      "component",
      "metric",
      "operation",
      "outcome",
      "unit",
      "value",
    ]);
    expect((parsed.fields as Record<string, unknown>).value).toBe(86_400_000);
  });

  test("drops invalid dimensions and recorder failures are non-fatal", () => {
    const lines: string[] = [];
    const recorder = new StructuredRuntimeRecorder({
      enabled: true,
      write: (line) => lines.push(line),
    });
    recorder.recordMetric({
      name: "browser.operation.duration",
      operation: "https://user:pass@example.invalid" as "navigation",
      outcome: "ok",
      unit: "milliseconds",
      value: 10,
    });
    expect(lines).toHaveLength(0);

    const failing = new StructuredRuntimeRecorder({
      enabled: true,
      write: () => {
        throw new Error("sink unavailable");
      },
    });
    expect(() =>
      failing.recordMetric({
        name: "browser.failure.count",
        operation: "launch",
        outcome: "error",
        unit: "count",
        value: 1,
      }),
    ).not.toThrow();
  });

  test("opt-out and noop paths do not write", () => {
    const lines: string[] = [];
    const disabled = new StructuredRuntimeRecorder({
      enabled: false,
      write: (line) => lines.push(line),
    });
    disabled.recordMetric({
      name: "browser.failure.count",
      operation: "launch",
      outcome: "error",
      unit: "count",
      value: 1,
    });
    new NoopRuntimeRecorder().recordMetric({
      name: "browser.failure.count",
      operation: "launch",
      outcome: "error",
      unit: "count",
      value: 1,
    });
    expect(lines).toHaveLength(0);
  });

  test("environment default is disabled without constructing any external sampler", () => {
    const previous = process.env.RETCON_OBSERVABILITY;
    delete process.env.RETCON_OBSERVABILITY;
    try {
      let writes = 0;
      const recorder = new StructuredRuntimeRecorder({ write: () => writes++ });
      recorder.recordMetric({
        name: "browser.operation.duration",
        operation: "navigation",
        outcome: "ok",
        unit: "milliseconds",
        value: 1,
      });
      expect(writes).toBe(0);
    } finally {
      if (previous === undefined) delete process.env.RETCON_OBSERVABILITY;
      else process.env.RETCON_OBSERVABILITY = previous;
    }
  });

  test("runtime privacy updates override the initial state", () => {
    const lines: string[] = [];
    const recorder = new StructuredRuntimeRecorder({
      enabled: false,
      write: (line) => lines.push(line),
    });
    recorder.setEnabled(true);
    recorder.recordMetric({
      name: "browser.failure.count",
      operation: "launch",
      outcome: "error",
      unit: "count",
      value: 1,
    });
    recorder.setEnabled(false);
    recorder.recordMetric({
      name: "browser.failure.count",
      operation: "launch",
      outcome: "error",
      unit: "count",
      value: 1,
    });
    expect(lines).toHaveLength(1);
  });
});
