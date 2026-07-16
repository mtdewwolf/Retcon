import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { rm } from "node:fs/promises";
import { join } from "node:path";
import { ManagedBrowser } from "../src/browser";

describe("managed Chromium", () => {
  test("navigates localhost, captures evidence, acts, and closes", async () => {
    if (process.env.RETCON_BROWSER_E2E !== "1") return;
    const server = Bun.serve({
      port: 0,
      fetch: () =>
        new Response("<button id='proof' onclick=\"console.log('clicked')\">proof</button>", {
          headers: { "content-type": "text/html" },
        }),
    });
    const events: string[] = [];
    const browser = new ManagedBrowser((event) => events.push(event.type));
    const screenshot = join(import.meta.dir, "browser-proof.png");
    try {
      await browser.launch();
      const navigation = await browser.navigate(`http://127.0.0.1:${server.port}`);
      expect(navigation.status).toBe(200);
      expect((await browser.action({ action: "text", selector: "#proof" })).text).toBe("proof");
      await browser.action({ action: "click", selector: "#proof" });
      await browser.screenshot({ path: screenshot });
      expect(existsSync(screenshot)).toBeTrue();
      expect((browser.logs().network as unknown[]).length).toBeGreaterThan(0);
      expect(events).toContain("browser.request");
    } finally {
      await browser.close();
      server.stop(true);
      await rm(screenshot, { force: true });
    }
    expect(browser.status().running).toBeFalse();
  }, 30_000);
});
