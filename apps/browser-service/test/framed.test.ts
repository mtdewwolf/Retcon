import { describe, expect, test } from "bun:test";
import { MAX_FRAME_BYTES, cappedLines } from "../src/framed.ts";

async function* chunksOf(...parts: string[]): AsyncGenerator<Buffer> {
  for (const part of parts) {
    yield Buffer.from(part);
  }
}

describe("cappedLines", () => {
  test("splits newline delimited frames across chunks", async () => {
    const lines: string[] = [];
    for await (const line of cappedLines(chunksOf('{"a":1}\n{"b":', "2}\n"))) {
      lines.push(line);
    }
    expect(lines).toEqual(['{"a":1}', '{"b":2}']);
  });

  test("rejects frames larger than the byte cap before newline", async () => {
    const oversized = `${"x".repeat(MAX_FRAME_BYTES + 8)}\n`;
    await expect(async () => {
      for await (const _line of cappedLines(chunksOf(oversized))) {
        // never yields
      }
    }).toThrow(`frame exceeds ${MAX_FRAME_BYTES} bytes`);
  });
});
