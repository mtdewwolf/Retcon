import { describe, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  boundedNumber,
  boundedText,
  identifier,
  safeInputPath,
  safeOutputPath,
  safeUrl,
  withTimeout,
} from "../src/validation";

describe("browser boundary validation", () => {
  test("bounds identifiers, numbers, URLs, output, and input paths", async () => {
    expect(identifier("session-1", "sessionId")).toBe("session-1");
    expect(() => identifier("../bad", "sessionId")).toThrow();
    expect(() => boundedNumber(Number.NaN, "value", 0, 10, 1)).toThrow();
    expect(() => safeUrl("javascript:alert(1)", [])).toThrow();
    expect(() => safeUrl("https://user:pass@example.com", [])).toThrow();
    const root = await mkdtemp(join(tmpdir(), "retcon-browser-paths-"));
    try {
      const input = join(root, "proof.txt");
      await writeFile(input, "proof");
      expect(await safeInputPath([root], input)).toBe(input);
      expect(await safeOutputPath(root, "nested/proof.png", "fallback.png")).toBe(
        join(root, "nested/proof.png"),
      );
      await expect(safeOutputPath(root, "../escape.png", "fallback.png")).rejects.toThrow();
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  test("bounds text and supports timeout/cancellation", async () => {
    expect(boundedText("abcdef", 3)).toEqual({ text: "abc", truncated: true });
    await expect(withTimeout(new Promise(() => undefined), 10)).rejects.toThrow("timed out");
    const controller = new AbortController();
    const pending = withTimeout(new Promise(() => undefined), 1_000, controller.signal);
    controller.abort();
    await expect(pending).rejects.toThrow("cancelled");
  });
});
