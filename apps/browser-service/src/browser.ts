import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Browser, type BrowserContext, type Page, chromium } from "playwright";

/** Retained console/network evidence per session (ring buffer). */
export const MAX_LOG_ENTRIES = 1_000;
/** Cap stored/emitted console text and URLs so count caps cannot still OOM. */
export const MAX_LOG_TEXT_CHARS = 8_192;

export interface BrowserEvent {
  type: string;
  payload: Record<string, unknown>;
}

export interface LogQuery {
  offset?: number;
  limit?: number;
}

export interface ScreenshotOptions {
  path: string;
  fullPage?: boolean;
  type?: "png" | "jpeg";
  quality?: number;
}

/** Truncate long console/network strings before buffering or emitting. */
export function truncateText(value: string, max = MAX_LOG_TEXT_CHARS): string {
  if (value.length <= max) return value;
  return `${value.slice(0, max)}…`;
}

/** Append to a ring buffer, dropping oldest entries when over capacity. */
export function pushCapped<T>(buffer: T[], entry: T, max: number): void {
  buffer.push(entry);
  if (buffer.length > max) {
    buffer.splice(0, buffer.length - max);
  }
}

function sliceLogs<T>(entries: T[], query: LogQuery = {}): T[] {
  const offset = Math.max(0, query.offset ?? 0);
  const limit = Math.min(Math.max(1, query.limit ?? 100), MAX_LOG_ENTRIES);
  return entries.slice(offset, offset + limit);
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
    if (this.context) return { alreadyRunning: true };
    this.consoleEntries.length = 0;
    this.networkEntries.length = 0;
    this.profile = await mkdtemp(join(tmpdir(), "retcon-browser-"));
    try {
      const executablePath = process.env.RETCON_CHROMIUM_PATH;
      this.context = await chromium.launchPersistentContext(this.profile, {
        headless: true,
        viewport: { width: 1280, height: 720 },
        ...(executablePath ? { executablePath } : {}),
      });
      this.browser = this.context.browser() ?? undefined;
      this.context.on("close", () => this.emit({ type: "browser.crashed", payload: {} }));
      this.page = this.context.pages()[0] ?? (await this.context.newPage());
      this.page.on("console", (message) => {
        const entry = {
          type: message.type(),
          text: truncateText(message.text()),
          timestamp: new Date().toISOString(),
        };
        pushCapped(this.consoleEntries, entry, MAX_LOG_ENTRIES);
        this.emit({ type: "browser.console", payload: entry });
      });
      this.page.on("request", (request) => {
        const entry = {
          method: request.method(),
          url: truncateText(request.url()),
          resourceType: request.resourceType(),
        };
        pushCapped(this.networkEntries, entry, MAX_LOG_ENTRIES);
        this.emit({ type: "browser.request", payload: entry });
      });
      this.page.on("response", (response) => {
        const entry = { status: response.status(), url: truncateText(response.url()) };
        pushCapped(this.networkEntries, entry, MAX_LOG_ENTRIES);
        this.emit({ type: "browser.response", payload: entry });
      });
      return { launched: true, profile: this.profile };
    } catch (error) {
      const context = this.context;
      this.browser = undefined;
      this.context = undefined;
      this.page = undefined;
      if (context) await context.close().catch(() => undefined);
      if (this.profile) await rm(this.profile, { recursive: true, force: true });
      this.profile = undefined;
      throw error;
    }
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

  async screenshot(options: ScreenshotOptions): Promise<Record<string, unknown>> {
    const page = this.requirePage();
    await page.screenshot({
      path: options.path,
      fullPage: options.fullPage ?? false,
      type: options.type ?? "png",
      ...(options.type === "jpeg" ? { quality: options.quality ?? 80 } : {}),
    });
    return { path: options.path, fullPage: options.fullPage ?? false };
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

  logs(query: LogQuery = {}): Record<string, unknown> {
    return {
      console: sliceLogs(this.consoleEntries, query),
      network: sliceLogs(this.networkEntries, query),
      totals: {
        console: this.consoleEntries.length,
        network: this.networkEntries.length,
      },
      offset: query.offset ?? 0,
      limit: query.limit ?? 100,
    };
  }

  status(): Record<string, unknown> {
    return {
      running: this.context !== undefined,
      url: this.page?.url() ?? null,
    };
  }

  async close(): Promise<Record<string, unknown>> {
    const context = this.context;
    this.browser = undefined;
    this.context = undefined;
    this.page = undefined;
    this.consoleEntries.length = 0;
    this.networkEntries.length = 0;
    if (context) await context.close();
    if (this.profile) await rm(this.profile, { recursive: true, force: true });
    this.profile = undefined;
    return { closed: true };
  }

  private requirePage(): Page {
    if (!this.page) throw new Error("managed browser is not running");
    return this.page;
  }
}
