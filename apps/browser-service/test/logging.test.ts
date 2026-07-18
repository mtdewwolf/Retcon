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
      log("error", "https://user:password@example.invalid/C:/secret", {
        answer: 42,
        error: "token=secret",
        version: "0.1.0",
        features: ["one", "two"],
      });
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
      fields: { message: string; version: string; featureCount: number };
    };
    expect(record.level).toBe("ERROR");
    expect(record.target).toBe("retcon_browser_service");
    expect(record.fields.message).toBe("browser service event");
    expect(record.fields.version).toBe("0.1.0");
    expect(record.fields.featureCount).toBe(2);
    expect(first).not.toContain("password");
    expect(first).not.toContain("secret");
    expect(first).not.toContain("example.invalid");
    expect(first).not.toContain("C:/");
    expect(Date.parse(record.timestamp)).not.toBeNaN();
  });
});
