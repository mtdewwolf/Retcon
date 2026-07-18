import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ManagedBrowser } from "../src/browser";

describe("managed Chromium", () => {
  test("reports managed installation and empty status", () => {
    const browser = new ManagedBrowser(() => undefined);
    expect(browser.installation().executablePath).toBeString();
    expect(browser.status().running).toBeFalse();
  });

  test("rejects invalid sessions and traversal before touching Chromium", async () => {
    const root = await mkdtemp(join(tmpdir(), "retcon-browser-unit-"));
    const browser = new ManagedBrowser(() => undefined, {
      artifactRoot: join(root, "artifacts"),
      profileRoot: join(root, "profiles"),
      inputRoots: [root],
    });
    try {
      await expect(browser.launch({ sessionId: "../escape" })).rejects.toThrow(
        "unsupported characters",
      );
      await expect(browser.navigate("javascript:alert(1)")).rejects.toThrow("not running");
    } finally {
      await browser.close();
      await rm(root, { recursive: true, force: true });
    }
  });

  test("runs isolated sessions, actions, evidence, traces, and cleanup against localhost", async () => {
    if (process.env.RETCON_BROWSER_E2E !== "1") return;
    const root = await mkdtemp(join(tmpdir(), "retcon-browser-e2e-"));
    const upload = join(root, "upload.txt");
    await writeFile(upload, "upload proof");
    const server = Bun.serve({
      port: 0,
      fetch(request) {
        const url = new URL(request.url);
        if (url.pathname === "/missing") return new Response("missing", { status: 404 });
        if (url.pathname === "/download") {
          return new Response("download proof", {
            headers: { "content-disposition": "attachment; filename=proof.txt" },
          });
        }
        if (url.pathname === "/second") {
          return new Response("<title>Second</title><h1 id='second'>second</h1>", {
            headers: { "content-type": "text/html" },
          });
        }
        return new Response(
          `<!doctype html><title>Fixture</title>
              <button id="proof" onclick="this.textContent='clicked'; console.log('clicked')">proof</button>
              <input id="name"><select id="choice"><option>a</option><option>b</option></select>
              <input id="file" type="file"><button id="dialog" onclick="alert('hello')">dialog</button>
              <a id="popup" target="_blank" href="/second">popup</a>
              <a id="download" href="/download" download>download</a>
              <div id="drag" draggable="true">drag</div><div id="drop">drop</div>
              <script>console.log('fixture-ready'); fetch('/missing');</script>`,
          {
            status: url.pathname === "/missing" ? 404 : 200,
            headers: { "content-type": "text/html" },
          },
        );
      },
    });
    const events: string[] = [];
    const browser = new ManagedBrowser((event) => events.push(event.type), {
      artifactRoot: join(root, "artifacts"),
      profileRoot: join(root, "profiles"),
      inputRoots: [root],
    });
    try {
      const launched = await browser.launch({ sessionId: "one", profileMode: "temporary" });
      expect(launched.running).toBeTrue();
      await browser.launch({ sessionId: "two", profileMode: "persistent", profileName: "two" });
      expect((browser.status().sessions as unknown[]).length).toBe(2);

      const base = `http://127.0.0.1:${server.port}`;
      const navigation = await browser.call("browser.navigate", { sessionId: "one", url: base });
      expect(navigation.status).toBe(200);
      await browser.call("browser.action", {
        sessionId: "one",
        action: "fill",
        selector: "#name",
        value: "Retcon",
      });
      await browser.call("browser.action", {
        sessionId: "one",
        action: "select",
        selector: "#choice",
        value: "b",
      });
      await browser.call("browser.action", {
        sessionId: "one",
        action: "upload",
        selector: "#file",
        paths: [upload],
      });
      await browser.call("browser.action", {
        sessionId: "one",
        action: "click",
        selector: "#proof",
      });
      expect(
        (
          await browser.call("browser.action", {
            sessionId: "one",
            action: "text",
            selector: "#proof",
          })
        ).text,
      ).toBe("clicked");
      const script = await browser.call("browser.action", {
        sessionId: "one",
        action: "script",
        script: "() => document.querySelector('#name').value",
      });
      expect(script.value).toBe("Retcon");

      await browser.call("browser.storage.set", {
        sessionId: "one",
        localStorage: { proof: "stored" },
        sessionStorage: { tab: "yes" },
      });
      expect(
        (await browser.call("browser.storage.get", { sessionId: "one" })).localStorage,
      ).toEqual({
        proof: "stored",
      });
      await browser.call("browser.cookies.set", {
        sessionId: "one",
        cookies: [{ name: "proof", value: "cookie", url: base }],
      });
      expect(
        ((await browser.call("browser.cookies.get", { sessionId: "one" })).cookies as unknown[])
          .length,
      ).toBe(1);
      await browser.call("browser.viewport.set", { sessionId: "one", width: 900, height: 600 });

      const popup = await browser.call("browser.action", {
        sessionId: "one",
        action: "popup",
        selector: "#popup",
      });
      expect(popup.url).toContain("/second");
      const tabs = await browser.call("browser.tab.list", { sessionId: "one" });
      expect((tabs.tabs as unknown[]).length).toBe(2);
      await browser.call("browser.tab.close", { sessionId: "one", pageId: popup.pageId });

      await browser.call("browser.trace.start", { sessionId: "one" });
      const snapshot = await browser.call("browser.snapshot", { sessionId: "one" });
      expect(snapshot.dom).toContain("Fixture");
      expect(snapshot.accessibility).toContain("proof");
      expect(
        (await browser.call("browser.performance", { sessionId: "one" })).navigation,
      ).toBeArray();
      const screenshot = await browser.call("browser.screenshot", {
        sessionId: "one",
        path: "screenshots/proof.png",
        fullPage: true,
      });
      expect(existsSync(screenshot.path as string)).toBeTrue();
      const trace = await browser.call("browser.trace.stop", {
        sessionId: "one",
        path: "traces/proof.zip",
      });
      expect(existsSync(trace.path as string)).toBeTrue();
      const download = await browser.call("browser.action", {
        sessionId: "one",
        action: "download",
        selector: "#download",
        path: "downloads/proof.txt",
      });
      expect(existsSync(download.path as string)).toBeTrue();

      const dialog = await browser.call("browser.action", {
        sessionId: "one",
        action: "dialog",
        selector: "#dialog",
        accept: true,
      });
      expect(dialog.message).toBe("hello");
      const paused = await browser.call("browser.pause", { sessionId: "one" });
      expect(paused.paused).toBeTrue();
      await expect(
        browser.call("browser.navigate", { sessionId: "one", url: `${base}/second` }),
      ).rejects.toThrow("paused");
      expect((await browser.call("browser.resume", { sessionId: "one" })).paused).toBeFalse();

      const logs = browser.logs({ sessionId: "one" });
      expect((logs.network as unknown[]).length).toBeGreaterThan(0);
      expect((logs.console as unknown[]).length).toBeGreaterThan(0);
      expect(events).toContain("browser.response");
      await expect(
        browser.call("browser.screenshot", { sessionId: "one", path: "../escape.png" }),
      ).rejects.toThrow("outside");
    } finally {
      await browser.close();
      server.stop(true);
      await rm(root, { recursive: true, force: true });
    }
    expect(browser.status().running).toBeFalse();
  }, 120_000);
});
