import { describe, expect, test } from "bun:test";
import { log } from "../src/logging";

describe("structured logging", () => {
  test("emits newline-delimited JSON with the shared shape", () => {
    const written: string[] = [];
    const original = process.stderr.write.bind(process.stderr);
    // @ts-expect-error — intercepting stderr for the assertion.
    process.stderr.write = (chunk: string) => {
      written.push(String(chunk));
      return true;
    };
    try {
      log("error", "test message", { answer: 42 });
    } finally {
      process.stderr.write = original;
    }

    expect(written).toHaveLength(1);
    const first = written[0];
    if (first === undefined) throw new Error("nothing was written");
    const record = JSON.parse(first) as {
      level: string;
      target: string;
      timestamp: string;
      fields: { message: string; answer: number };
    };
    expect(record.level).toBe("ERROR");
    expect(record.target).toBe("retcon_browser_service");
    expect(record.fields.message).toBe("test message");
    expect(record.fields.answer).toBe(42);
    expect(Date.parse(record.timestamp)).not.toBeNaN();
  });
});
