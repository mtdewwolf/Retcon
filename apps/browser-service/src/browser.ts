import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Browser, type BrowserContext, type Page, chromium } from "playwright";

/** Retained console/network evidence per session (ring buffer). */
export const MAX_LOG_ENTRIES = 1_000;
const MAX_TEXT_CHARS = 8_192;
const MAX_URL_CHARS = 2_048;

export interface BrowserEvent {
  type: string;
  payload: Record<string, unknown>;
}

export interface LogQuery {
  offset?: number;
  limit?: number;
  /** When true (default), return the newest page of entries. */
  tail?: boolean;
}

export interface ScreenshotOptions {
  path: string;
  fullPage?: boolean;
  type?: "png" | "jpeg";
  quality?: number;
}

/** Append to a ring buffer, dropping oldest entries when over capacity. */
export function pushCapped<T>(buffer: T[], entry: T, max: number): void {
  buffer.push(entry);
  if (buffer.length > max) {
    buffer.splice(0, buffer.length - max);
  }
}

function truncate(value: string, max: number): string {
  return value.length > max ? `${value.slice(0, max)}…` : value;
}

function sliceLogs<T>(entries: T[], query: LogQuery = {}): T[] {
  const limit = Math.min(Math.max(1, query.limit ?? 100), MAX_LOG_ENTRIES);
  if (query.tail !== false && query.offset === undefined) {
    return entries.slice(Math.max(0, entries.length - limit));
  }
  const offset = Math.max(0, query.offset ?? 0);
  return entries.slice(offset, offset + limit);
}

export class ManagedBrowser {
  private browser: Browser | undefined;
  private context: BrowserContext | undefined;
  private page: Page | undefined;
  private profile: string | undefined;
  private closingIntentionally = false;
  private readonly consoleEntries: Record<string, unknown>[] = [];
  private readonly networkEntries: Record<string, unknown>[] = [];

  constructor(private readonly emit: (event: BrowserEvent) => void) {}

  async launch(): Promise<Record<string, unknown>> {
    if (this.context) return { alreadyRunning: true };
    this.closingIntentionally = false;
    this.profile = await mkdtemp(join(tmpdir(), "retcon-browser-"));
    const executablePath = process.env.RETCON_CHROMIUM_PATH;
    this.context = await chromium.launchPersistentContext(this.profile, {
      headless: true,
      viewport: { width: 1280, height: 720 },
      ...(executablePath ? { executablePath } : {}),
    });
    this.browser = this.context.browser() ?? undefined;
    this.context.on("close", () => {
      this.browser = undefined;
      this.context = undefined;
      this.page = undefined;
      if (!this.closingIntentionally) {
        this.emit({ type: "browser.crashed", payload: {} });
      }
      this.closingIntentionally = false;
    });
    this.page = this.context.pages()[0] ?? (await this.context.newPage());
    this.page.on("console", (message) => {
      const entry = {
        type: message.type(),
        text: truncate(message.text(), MAX_TEXT_CHARS),
        timestamp: new Date().toISOString(),
      };
      pushCapped(this.consoleEntries, entry, MAX_LOG_ENTRIES);
      this.emit({ type: "browser.console", payload: entry });
    });
    this.page.on("request", (request) => {
      const entry = {
        method: request.method(),
        url: truncate(request.url(), MAX_URL_CHARS),
        resourceType: request.resourceType(),
      };
      pushCapped(this.networkEntries, entry, MAX_LOG_ENTRIES);
      this.emit({ type: "browser.request", payload: entry });
    });
    this.page.on("response", (response) => {
      const entry = {
        status: response.status(),
        url: truncate(response.url(), MAX_URL_CHARS),
      };
      pushCapped(this.networkEntries, entry, MAX_LOG_ENTRIES);
      this.emit({ type: "browser.response", payload: entry });
    });
    return { launched: true, profile: this.profile };
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
    return {
      url: truncate(page.url(), MAX_URL_CHARS),
      title: truncate(await page.title(), MAX_TEXT_CHARS),
      status: response?.status() ?? null,
    };
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
        return { text: truncate(await page.locator(selector).innerText(), MAX_TEXT_CHARS) };
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
      tail: query.tail !== false && query.offset === undefined,
    };
  }

  status(): Record<string, unknown> {
    return {
      running: this.context !== undefined,
      url: this.page?.url() ? truncate(this.page.url(), MAX_URL_CHARS) : null,
    };
  }

  async close(): Promise<Record<string, unknown>> {
    const context = this.context;
    const profile = this.profile;
    this.closingIntentionally = true;
    this.browser = undefined;
    this.context = undefined;
    this.page = undefined;
    this.profile = undefined;
    if (context) await context.close();
    if (profile) await rm(profile, { recursive: true, force: true });
    return { closed: true };
  }

  private requirePage(): Page {
    if (!this.page) throw new Error("managed browser is not running");
    return this.page;
  }
}
