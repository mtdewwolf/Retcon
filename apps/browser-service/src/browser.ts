import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Browser, type BrowserContext, type Page, chromium } from "playwright";

export interface BrowserEvent {
  type: string;
  payload: Record<string, unknown>;
}

export class ManagedBrowser {
  private browser: Browser | undefined;
  private context: BrowserContext | undefined;
  private page: Page | undefined;
  private profile: string | undefined;
  private readonly consoleEntries: Record<string, unknown>[] = [];
  private readonly networkEntries: Record<string, unknown>[] = [];

  constructor(private readonly emit: (event: BrowserEvent) => void) {}

  async launch(): Promise<Record<string, unknown>> {
    if (this.browser?.isConnected()) return { alreadyRunning: true };
    this.profile = await mkdtemp(join(tmpdir(), "retcon-browser-"));
    const executablePath = process.env.RETCON_CHROMIUM_PATH;
    this.browser = await chromium.launch({
      headless: true,
      ...(executablePath ? { executablePath } : {}),
    });
    this.browser.on("disconnected", () => this.emit({ type: "browser.crashed", payload: {} }));
    this.context = await this.browser.newContext({ viewport: { width: 1280, height: 720 } });
    this.page = await this.context.newPage();
    this.page.on("console", (message) => {
      const entry = {
        type: message.type(),
        text: message.text(),
        timestamp: new Date().toISOString(),
      };
      this.consoleEntries.push(entry);
      this.emit({ type: "browser.console", payload: entry });
    });
    this.page.on("request", (request) => {
      const entry = {
        method: request.method(),
        url: request.url(),
        resourceType: request.resourceType(),
      };
      this.networkEntries.push(entry);
      this.emit({ type: "browser.request", payload: entry });
    });
    this.page.on("response", (response) =>
      this.emit({
        type: "browser.response",
        payload: { status: response.status(), url: response.url() },
      }),
    );
    return { launched: true };
  }

  async navigate(url: string): Promise<Record<string, unknown>> {
    const page = this.requirePage();
    const parsed = new URL(url);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:")
      throw new Error("only HTTP(S) URLs are allowed");
    const response = await page.goto(parsed.toString(), {
      waitUntil: "domcontentloaded",
      timeout: 30_000,
    });
    return { url: page.url(), title: await page.title(), status: response?.status() ?? null };
  }

  async screenshot(path: string): Promise<Record<string, unknown>> {
    await this.requirePage().screenshot({ path, fullPage: true });
    return { path };
  }

  async action(params: Record<string, unknown>): Promise<Record<string, unknown>> {
    const page = this.requirePage();
    const selector = String(params.selector ?? "");
    switch (params.action) {
      case "click":
        await page.locator(selector).click();
        break;
      case "fill":
        await page.locator(selector).fill(String(params.value ?? ""));
        break;
      case "press":
        await page.locator(selector).press(String(params.value ?? "Enter"));
        break;
      case "text":
        return { text: await page.locator(selector).innerText() };
      default:
        throw new Error(`unsupported action: ${String(params.action)}`);
    }
    return { completed: true };
  }

  logs(): Record<string, unknown> {
    return { console: this.consoleEntries, network: this.networkEntries };
  }
  status(): Record<string, unknown> {
    return { running: this.browser?.isConnected() ?? false, url: this.page?.url() ?? null };
  }

  async close(): Promise<Record<string, unknown>> {
    const browser = this.browser;
    this.browser = undefined;
    this.context = undefined;
    this.page = undefined;
    if (browser) await browser.close();
    if (this.profile) await rm(this.profile, { recursive: true, force: true });
    this.profile = undefined;
    return { closed: true };
  }

  private requirePage(): Page {
    if (!this.page) throw new Error("managed browser is not running");
    return this.page;
  }
}
