import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ManagedBrowser } from "../src/browser.ts";

const root = await mkdtemp(join(tmpdir(), "retcon-browser-node-e2e-"));
const server = createServer((request, response) => {
  if (request.url === "/next") {
    response.writeHead(200, { "content-type": "text/html" });
    response.end("<title>Next</title><h1>next</h1>");
    return;
  }
  response.writeHead(200, { "content-type": "text/html" });
  response.end(`<!doctype html><title>Retcon fixture</title>
    <button id="proof" onclick="this.textContent='clicked'; console.log('clicked')">proof</button>
    <input id="name"><a id="next" href="/next">next</a>
    <script>console.log('fixture-ready')</script>`);
});
await new Promise<void>((resolve, reject) => {
  server.once("error", reject);
  server.listen(0, "127.0.0.1", resolve);
});
const address = server.address();
assert(address && typeof address === "object");
const base = `http://127.0.0.1:${address.port}`;
const events: string[] = [];
const browser = new ManagedBrowser((event) => events.push(event.type), {
  artifactRoot: join(root, "artifacts"),
  profileRoot: join(root, "profiles"),
  inputRoots: [root],
});

try {
  const installation = browser.installation();
  assert.equal(installation.installed, true);
  const launch = await browser.launch({
    sessionId: "smoke",
    profileMode: "persistent",
    profileName: "smoke",
  });
  assert.equal(launch.running, true);
  assert.equal(typeof launch.browserVersion, "string");
  const navigation = await browser.call("browser.navigate", { sessionId: "smoke", url: base });
  assert.equal(navigation.status, 200);
  await browser.call("browser.action", {
    sessionId: "smoke",
    action: "fill",
    selector: "#name",
    value: "Retcon",
  });
  await browser.call("browser.action", {
    sessionId: "smoke",
    action: "click",
    selector: "#proof",
  });
  const text = await browser.call("browser.action", {
    sessionId: "smoke",
    action: "text",
    selector: "#proof",
  });
  assert.equal(text.text, "clicked");
  await browser.call("browser.storage.set", {
    sessionId: "smoke",
    localStorage: { proof: "stored" },
  });
  const snapshot = await browser.call("browser.snapshot", { sessionId: "smoke" });
  assert.match(String(snapshot.accessibility), /clicked/);
  await browser.call("browser.trace.start", { sessionId: "smoke" });
  const screenshot = await browser.call("browser.screenshot", {
    sessionId: "smoke",
    path: "service-smoke.png",
    fullPage: true,
  });
  assert(existsSync(String(screenshot.path)));
  const trace = await browser.call("browser.trace.stop", {
    sessionId: "smoke",
    path: "service-smoke-trace.zip",
  });
  assert(existsSync(String(trace.path)));
  const tab = await browser.call("browser.tab.new", { sessionId: "smoke", url: `${base}/next` });
  assert.match(String(tab.url), /next/);
  assert.equal((browser.logs({ sessionId: "smoke" }).console as unknown[]).length > 0, true);
  assert(events.includes("browser.response"));
  const paused = await browser.call("browser.pause", { sessionId: "smoke" });
  assert.equal(paused.paused, true);
  await browser.call("browser.resume", { sessionId: "smoke" });
  const closed = await browser.close({ sessionId: "smoke" });
  assert.equal(closed.closed, true);
  const recovered = await browser.call("browser.recover", { sessionId: "smoke" });
  assert.equal(recovered.recovered, true);
  await browser.close({ sessionId: "smoke", deleteProfile: true });
  console.log(JSON.stringify({ ok: true, browserVersion: launch.browserVersion, artifacts: root }));
} finally {
  await browser.close();
  await new Promise<void>((resolve) => server.close(() => resolve()));
  await rm(root, { recursive: true, force: true });
}
