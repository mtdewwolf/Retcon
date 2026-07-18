import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { ManagedBrowser } from "../src/browser.ts";
import {
  type BrowserVerificationResult,
  MAX_VERIFICATION_SCREENSHOT_BYTES,
} from "../src/verification.ts";

const root = await mkdtemp(join(tmpdir(), "retcon-verification-node-e2e-"));
let mode: "stable" | "changed" | "trap" = "stable";
const events: string[] = [];
const server = createServer((_request, response) => {
  response.writeHead(200, { "content-type": "text/html" });
  response.end(`<!doctype html>
    <html><head><title>Verification fixture</title>
    <style>
      body { background: white; color: black; font: 16px sans-serif; }
      main { padding: 24px; border: 20px solid ${mode === "changed" ? "rgb(200, 0, 0)" : "rgb(0, 80, 180)"}; }
      :focus-visible { outline: 3px solid rgb(0, 80, 180); }
    </style></head><body><main><h1>Verification</h1>
    <form aria-label="Greeting form" onsubmit="event.preventDefault(); document.querySelector('#result').textContent='Hello '+document.querySelector('#name').value; console.log('submitted')">
      <label for="name">Name</label><input id="name" name="name">
      <button id="submit" type="submit">Submit</button>
    </form>
    <p id="result" aria-live="polite">Waiting</p><p class="dynamic">${Date.now()}</p>
    <p class="ignored">${Math.random()}</p>
    ${mode === "trap" ? "<script>document.addEventListener('keydown', event => { if (event.key === 'Tab') { event.preventDefault(); document.querySelector('#submit').focus(); } });</script>" : ""}
    </main></body></html>`);
});

await new Promise<void>((resolvePromise) => server.listen(0, "127.0.0.1", resolvePromise));
const address = server.address();
if (!address || typeof address === "string") throw new Error("fixture server did not bind");
const targetUrl = `http://127.0.0.1:${address.port}?token=verification-secret`;
const browser = new ManagedBrowser((event) => events.push(event.type), {
  artifactRoot: join(root, "artifacts"),
  profileRoot: join(root, "profiles"),
  inputRoots: [root],
});

async function authenticatedStdioSmoke(): Promise<void> {
  const serviceDirectory = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const child = spawn(process.execPath, ["src/main.ts", "--stdio"], {
    cwd: serviceDirectory,
    env: {
      ...process.env,
      RETCON_BROWSER_AUTH_TOKEN: "verification-e2e-token",
      RETCON_BROWSER_ARTIFACT_ROOT: join(root, "stdio-artifacts"),
      RETCON_BROWSER_PROFILE_ROOT: join(root, "stdio-profiles"),
      RETCON_BROWSER_INPUT_ROOTS: root,
    },
    stdio: ["pipe", "pipe", "pipe"],
  });
  const lines = createInterface({ input: child.stdout, crlfDelay: Number.POSITIVE_INFINITY });
  const pending = new Map<
    number,
    {
      resolve: (value: Record<string, unknown>) => void;
      reject: (error: Error) => void;
      timer: ReturnType<typeof setTimeout>;
    }
  >();
  let serviceStderr = "";
  child.stderr.on("data", (chunk: Buffer) => {
    serviceStderr += chunk.toString("utf8");
  });
  let requestId = 0;
  lines.on("line", (line) => {
    const message = JSON.parse(line) as {
      id?: number;
      result?: Record<string, unknown>;
      error?: { code?: string; message?: string };
    };
    if (message.id === undefined) return;
    const waiting = pending.get(message.id);
    if (!waiting) return;
    pending.delete(message.id);
    clearTimeout(waiting.timer);
    if (message.error) {
      waiting.reject(new Error(`${message.error.code ?? "service_error"}: ${message.error.message ?? "request failed"}`));
    } else waiting.resolve(message.result ?? {});
  });
  const call = (method: string, params: Record<string, unknown>): Promise<Record<string, unknown>> => {
    requestId += 1;
    const id = requestId;
    const response = new Promise<Record<string, unknown>>((resolvePromise, reject) => {
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(
          new Error(
            `${method} stdio response timed out; stderr=${serviceStderr.slice(-4_096)}`,
          ),
        );
      }, method === "browser.verification.run" ? 150_000 : 15_000);
      pending.set(id, { resolve: resolvePromise, reject, timer });
    });
    child.stdin.write(`${JSON.stringify({ id, method, params })}\n`);
    return response;
  };
  try {
    await assert.rejects(
      call("browser.verification.run", request("stdio-unauthorized", "never")),
      /unauthorized/,
    );
    const hello = await call("service.hello", {
      token: "verification-e2e-token",
      protocolVersion: 1,
    });
    assert.ok((hello.features as string[]).includes("verification"));
    const result = (await call("browser.verification.run", {
      runId: "stdio-smoke",
      timeoutMs: 120_000,
      definition: {
        id: "stdio-smoke",
        targetUrl,
        serverRef: "fixture-server",
        failOnAccessibility: true,
        steps: [{ id: "status", type: "assert.status", expected: 200 }],
      },
    })) as unknown as BrowserVerificationResult;
    assert.equal(
      result.status,
      "passed",
      JSON.stringify({ variants: result.variants, accessibility: result.accessibility }),
    );
    await call("service.shutdown", {});
    if (child.exitCode === null) {
      await new Promise<void>((resolvePromise, reject) => {
        child.once("exit", (code) =>
          code === 0 ? resolvePromise() : reject(new Error(`service exited with ${String(code)}`)),
        );
      });
    } else assert.equal(child.exitCode, 0);
  } finally {
    child.stdin.end();
    lines.close();
    if (child.exitCode === null) child.kill();
  }
}

function request(
  runId: string,
  updateBaseline: "never" | "missing" | "always",
  options: { screenshots?: boolean; failOnAccessibility?: boolean } = {},
) {
  const steps: Record<string, unknown>[] = [
    { id: "status", type: "assert.status", expected: 200 },
    { id: "fill", type: "fill", selector: "#name", value: "Retcon" },
    { id: "submit", type: "click", selector: "button" },
    {
      id: "greeting",
      type: "assert.text",
      selector: "#result",
      expected: "Hello Retcon",
      mode: "equals",
    },
    { id: "visible", type: "assert.element", selector: "main", state: "visible" },
    { id: "console", type: "assert.console", text: "submitted" },
  ];
  if (options.screenshots !== false) {
    steps.push({ id: "evidence", type: "screenshot", name: "completed", fullPage: true });
  }
  return {
    runId,
    timeoutMs: 120_000,
    definition: {
      id: "fixture-flow",
      targetUrl,
      serverRef: "fixture-server",
      timeoutMs: 30_000,
      retries: 0,
      failOnAccessibility: options.failOnAccessibility ?? true,
      variants:
        options.screenshots === false
          ? [{ name: "desktop", width: 1024, height: 768 }]
          : [
              { name: "desktop", width: 1024, height: 768 },
              { name: "mobile", width: 390, height: 844, isMobile: true },
            ],
      visual: {
        updateBaseline,
        pixelThreshold: 0.05,
        maxDiffPixelRatio: 0,
        perceptualThreshold: 0,
        maskSelectors: [".dynamic"],
        ignoreSelectors: [".ignored"],
      },
      steps,
    },
  };
}

try {
  const firstUnapproved = (await browser.call(
    "browser.verification.run",
    request("first-unapproved", "never"),
  )) as unknown as BrowserVerificationResult;
  assert.equal(firstUnapproved.status, "passed");
  assert.ok(
    firstUnapproved.visualComparisons.every((comparison) => comparison.status === "created"),
  );
  assert.ok(
    firstUnapproved.visualComparisons.every((comparison) => comparison.baselinePath.length > 0),
  );

  const created = (await browser.call(
    "browser.verification.run",
    request("baseline", "missing"),
  )) as unknown as BrowserVerificationResult;
  assert.equal(created.status, "passed");
  assert.equal(created.variants.length, 2);
  assert.ok(created.visualComparisons.every((comparison) => comparison.status === "created"));
  assert.ok(created.assertions.every((assertion) => assertion.status === "passed"));
  assert.ok(created.console.entries.some((entry) => entry.text.includes("submitted")));
  assert.ok(created.network.entries.some((entry) => entry.kind === "response"));
  assert.ok(!JSON.stringify(created).includes("verification-secret"));
  assert.equal(created.accessibility.flatMap((audit) => audit.findings).length, 0);
  assert.deepEqual(
    created.timeline.map((event) => event.sequence),
    created.timeline.map((_event, index) => index + 1),
  );
  assert.ok(created.artifacts.every((artifact) => artifact.bytes <= MAX_VERIFICATION_SCREENSHOT_BYTES));

  const matching = (await browser.call(
    "browser.verification.run",
    request("matching", "never"),
  )) as unknown as BrowserVerificationResult;
  assert.equal(matching.status, "passed");
  assert.ok(matching.visualComparisons.every((comparison) => comparison.diffPixelRatio === 0));

  mode = "changed";
  const changed = (await browser.call(
    "browser.verification.run",
    request("changed", "never"),
  )) as unknown as BrowserVerificationResult;
  assert.equal(changed.status, "failed");
  assert.ok(changed.visualComparisons.some((comparison) => comparison.status === "failed"));
  assert.ok(changed.artifacts.some((artifact) => artifact.kind === "visual-diff"));
  assert.ok(changed.assertions.some((assertion) => assertion.status === "failed"));

  const updated = (await browser.call(
    "browser.verification.run",
    request("updated", "always"),
  )) as unknown as BrowserVerificationResult;
  assert.equal(updated.status, "passed");
  assert.ok(updated.visualComparisons.every((comparison) => comparison.status === "updated"));
  assert.ok(updated.visualComparisons.every((comparison) => comparison.baselineHistory.length > 0));

  mode = "trap";
  const trapped = (await browser.call(
    "browser.verification.run",
    request("keyboard-trap", "never", { screenshots: false, failOnAccessibility: true }),
  )) as unknown as BrowserVerificationResult;
  assert.equal(trapped.status, "failed");
  assert.ok(
    trapped.accessibility
      .flatMap((audit) => audit.findings)
      .some((finding) => finding.rule === "keyboard" && finding.message.includes("trapped")),
  );
  assert.ok(events.includes("browser.verification.completed"));
  mode = "stable";
  await authenticatedStdioSmoke();
  console.log(
    JSON.stringify({
      ok: true,
      variants: created.variants.map((variant) => variant.name),
      visualDiffRatio: changed.visualComparisons[0]?.diffPixelRatio,
      keyboardTrapDetected: true,
      artifacts: root,
    }),
  );
} finally {
  await browser.close();
  await new Promise<void>((resolvePromise, reject) =>
    server.close((error) => (error ? reject(error) : resolvePromise())),
  );
  await rm(root, { recursive: true, force: true });
}
