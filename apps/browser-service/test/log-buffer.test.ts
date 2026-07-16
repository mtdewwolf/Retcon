import { describe, expect, test } from "bun:test";
import { MAX_LOG_ENTRIES, pushCapped, truncateField } from "../src/browser";

describe("browser log ring buffer", () => {
  test("drops oldest entries once capacity is exceeded", () => {
    const buffer: number[] = [];
    for (let i = 0; i < MAX_LOG_ENTRIES + 50; i += 1) {
      pushCapped(buffer, i, MAX_LOG_ENTRIES);
    }
    expect(buffer).toHaveLength(MAX_LOG_ENTRIES);
    expect(buffer[0]).toBe(50);
    expect(buffer[buffer.length - 1]).toBe(MAX_LOG_ENTRIES + 49);
  });

  test("truncates oversized evidence fields", () => {
    const long = "x".repeat(5_000);
    const truncated = truncateField(long, 100);
    expect(truncated.length).toBeLessThan(long.length);
    expect(truncated.startsWith("x".repeat(100))).toBeTrue();
    expect(truncated).toContain("truncated");
  });
});
