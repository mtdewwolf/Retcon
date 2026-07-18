import { describe, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  BrowserVerificationRunner,
  MAX_VERIFICATION_SCREENSHOT_BYTES,
  enforceVerificationArtifactSize,
  parseVerificationRequest,
} from "../src/verification";

describe("browser verification contract", () => {
  test("validates bounded self-contained definitions and ratio thresholds", () => {
    const parsed = parseVerificationRequest(
      {
        runId: "run-1",
        definition: {
          id: "checkout",
          targetUrl: "http://127.0.0.1:3000",
          serverRef: "dev-server-1",
          steps: [{ type: "assert.status", expected: 200 }],
          visual: { maxDiffPixelRatio: 0.02, perceptualThreshold: 0.01 },
        },
      },
      [],
    );
    expect(parsed.definition.visual.maxDiffPixelRatio).toBe(0.02);
    expect(parsed.definition.visual.perceptualThreshold).toBe(0.01);
    expect(parsed.definition.variants[0]?.name).toBe("desktop");

    expect(() =>
      parseVerificationRequest(
        {
          definition: {
            id: "checkout",
            targetUrl: "http://127.0.0.1:3000",
            steps: [{ type: "assert.status", expected: 200 }],
          },
        },
        [],
      ),
    ).toThrow("serverRef");
    expect(() =>
      parseVerificationRequest(
        {
          definition: {
            id: "checkout",
            targetUrl: "http://127.0.0.1:3000",
            serverRef: "dev-server-1",
            steps: [{ type: "assert.status", expected: 200 }],
            visual: { maxDiffPixelRatio: 101 },
          },
        },
        [],
      ),
    ).toThrow("between 0 and 1");
  });

  test("rejects artifact traversal before launching Chromium", async () => {
    const root = await mkdtemp(join(tmpdir(), "retcon-verification-path-"));
    const runner = new BrowserVerificationRunner({
      artifactRoot: join(root, "artifacts"),
      inputRoots: [root],
      emit: () => undefined,
    });
    try {
      await expect(
        runner.run({
          runId: "escape",
          artifactDirectory: "../escape",
          definition: {
            id: "escape",
            targetUrl: "http://127.0.0.1:3000",
            serverRef: "dev-server-1",
            steps: [{ type: "assert.status", expected: 200 }],
          },
        }),
      ).rejects.toThrow("managed artifact root");
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  test("rejects evidence above the Core 16 MiB artifact boundary", async () => {
    const root = await mkdtemp(join(tmpdir(), "retcon-verification-size-"));
    const oversized = join(root, "oversized.png");
    try {
      await writeFile(oversized, Buffer.alloc(MAX_VERIFICATION_SCREENSHOT_BYTES + 1));
      await expect(enforceVerificationArtifactSize(oversized)).rejects.toThrow("artifact exceeds");
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  test("honors cancellation before browser state is touched", async () => {
    const root = await mkdtemp(join(tmpdir(), "retcon-verification-cancel-"));
    const controller = new AbortController();
    controller.abort();
    const runner = new BrowserVerificationRunner({
      artifactRoot: join(root, "artifacts"),
      inputRoots: [root],
      emit: () => undefined,
    });
    try {
      const result = await runner.run(
        {
          runId: "cancelled",
          definition: {
            id: "cancelled",
            targetUrl: "http://127.0.0.1:3000",
            serverRef: "dev-server-1",
            steps: [{ type: "assert.status", expected: 200 }],
          },
        },
        controller.signal,
      );
      expect(result.status).toBe("cancelled");
      expect(result.artifacts).toHaveLength(0);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});
