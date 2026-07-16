import { describe, expect, test } from "bun:test";
import { MAX_LOG_ENTRIES, pushCapped } from "../src/browser";

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
});
