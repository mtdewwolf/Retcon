import { describe, expect, test } from "bun:test";
import { MAX_LOG_ENTRIES, MAX_LOG_TEXT_CHARS, pushCapped, truncateText } from "../src/browser";

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

  test("truncates oversized console and network strings", () => {
    const long = "x".repeat(MAX_LOG_TEXT_CHARS + 100);
    const truncated = truncateText(long);
    expect(truncated.length).toBe(MAX_LOG_TEXT_CHARS + 1);
    expect(truncated.endsWith("…")).toBe(true);
    expect(truncateText("short")).toBe("short");
  });
});
