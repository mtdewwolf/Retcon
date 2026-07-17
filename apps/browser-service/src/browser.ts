import { existsSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  type BrowserContext,
  type BrowserContextOptions,
  type Cookie,
  type Page,
  chromium,
  devices,
} from "playwright";
import {
  MAX_SCRIPT_BYTES,
  MAX_TEXT_BYTES,
  booleanParam,
  boundedJson,
  boundedNumber,
  boundedText,
  enforceFileSize,
  ensureRoot,
  identifier,
  isWithin,
  objectParam,
  safeInputPath,
  safeOutputPath,
  safeUrl,
  stringParam,
  timeoutParam,
  withTimeout,
} from "./validation.ts";

export const MAX_LOG_ENTRIES = 1_000;
const MAX_COOKIES = 500;
const DEFAULT_VIEWPORT = { width: 1280, height: 720 };

export interface BrowserEvent {
  type: string;
  payload: Record<string, unknown>;
}

export interface LogQuery {
  offset?: number;
  limit?: number;
  sessionId?: string;
}

export interface ScreenshotOptions {
  path?: string;
  fullPage?: boolean;
  type?: "png" | "jpeg";
  quality?: number;
  sessionId?: string;
}

export interface ManagedBrowserOptions {
  artifactRoot?: string;
  profileRoot?: string;
  inputRoots?: string[];
}

interface LaunchConfig {
  sessionId: string;
  profileMode: "temporary" | "persistent";
  profileName: string;
  headless: boolean;
  viewport: { width: number; height: number };
  userAgent?: string;
  deviceName?: string;
  recordVideo: boolean;
}

interface AuditInterval {
  openedAt: string;
  closedAt?: string;
  mode: "paused" | "headed";
}

interface Session {
  id: string;
  context: BrowserContext;
  pages: Map<string, Page>;
  pageIds: WeakMap<Page, string>;
  activePageId: string;
  nextPage: number;
  profilePath: string;
  temporaryProfile: boolean;
  config: LaunchConfig;
  consoleEntries: Record<string, unknown>[];
  networkEntries: Record<string, unknown>[];
  pageErrors: Record<string, unknown>[];
  expectedClose: boolean;
  paused: boolean;
  audit: AuditInterval[];
  tracing: boolean;
}

export function pushCapped<T>(buffer: T[], entry: T, max: number): void {
  buffer.push(entry);
  if (buffer.length > max) buffer.splice(0, buffer.length - max);
}

function sliceLogs<T>(entries: T[], query: LogQuery = {}): T[] {
  const offset = Math.max(0, Math.trunc(query.offset ?? 0));
  const limit = Math.min(Math.max(1, Math.trunc(query.limit ?? 100)), MAX_LOG_ENTRIES);
  return entries.slice(offset, offset + limit);
}

export class ManagedBrowser {
  private readonly emit: (event: BrowserEvent) => void;
  private readonly sessions = new Map<string, Session>();
  private readonly recoverable = new Map<string, LaunchConfig>();
  private readonly artifactRoot: string;
  private readonly profileRoot: string;
  private readonly inputRoots: string[];
  private rootsReady: Promise<void>;

  constructor(emit: (event: BrowserEvent) => void, options: ManagedBrowserOptions = {}) {
    this.emit = emit;
    const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
    this.artifactRoot = resolve(options.artifactRoot ?? join(repositoryRoot, "output/playwright"));
    this.profileRoot = resolve(
      options.profileRoot ?? join(repositoryRoot, "output/browser-profiles"),
    );
    this.inputRoots = (options.inputRoots ?? [repositoryRoot]).map((root) => resolve(root));
    this.rootsReady = Promise.all([
      ensureRoot(this.artifactRoot),
      ensureRoot(this.profileRoot),
      ...this.inputRoots.map((root) => ensureRoot(root)),
    ]).then(() => undefined);
  }

  installation(): Record<string, unknown> {
    const executablePath = process.env.RETCON_CHROMIUM_PATH ?? chromium.executablePath();
    return {
      installed: existsSync(executablePath),
      executablePath,
      playwrightVersion: "1.61.1",
    };
  }

  async call(
    method: string,
    rawParams: Record<string, unknown> = {},
    signal?: AbortSignal,
  ): Promise<Record<string, unknown>> {
    const params = objectParam(rawParams);
    const timeout = timeoutParam(params.timeoutMs);
    const operation = this.dispatch(method, params, signal);
    try {
      return await withTimeout(operation, timeout, signal);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (message.includes("timed out") || message.includes("cancelled")) {
        const sessionId =
          params.sessionId === undefined ? "default" : identifier(params.sessionId, "sessionId");
        const session = this.sessions.get(sessionId);
        const page = session?.pages.get(session.activePageId);
        if (page && !page.isClosed()) await page.close().catch(() => undefined);
        if (session && session.pages.size === 0 && !session.expectedClose) {
          this.attachPage(session, await session.context.newPage());
        }
        this.emit({ type: "browser.operation.cancelled", payload: { sessionId, method, message } });
      }
      throw error;
    }
  }

  async launch(params: Record<string, unknown> = {}): Promise<Record<string, unknown>> {
    return this.call("browser.launch", params);
  }

  async navigate(url: string): Promise<Record<string, unknown>> {
    return this.call("browser.navigate", { url });
  }

  async screenshot(options: ScreenshotOptions): Promise<Record<string, unknown>> {
    return this.call("browser.screenshot", { ...options });
  }

  async action(params: Record<string, unknown>): Promise<Record<string, unknown>> {
    return this.call("browser.action", params);
  }

  logs(query: LogQuery = {}): Record<string, unknown> {
    const session = this.requireSession(query.sessionId);
    return {
      console: sliceLogs(session.consoleEntries, query),
      network: sliceLogs(session.networkEntries, query),
      pageErrors: sliceLogs(session.pageErrors, query),
      totals: {
        console: session.consoleEntries.length,
        network: session.networkEntries.length,
        pageErrors: session.pageErrors.length,
      },
      offset: query.offset ?? 0,
      limit: query.limit ?? 100,
    };
  }

  status(sessionId?: string): Record<string, unknown> {
    if (!sessionId) {
      const sessions = [...this.sessions.values()].map((session) => this.sessionStatus(session));
      return {
        running: sessions.length > 0,
        url: sessions[0]?.url ?? null,
        sessions,
        installation: this.installation(),
      };
    }
    const session = this.sessions.get(sessionId);
    return session ? this.sessionStatus(session) : { running: false, sessionId };
  }

  async close(params: Record<string, unknown> = {}): Promise<Record<string, unknown>> {
    const sessionId =
      params.sessionId === undefined ? undefined : identifier(params.sessionId, "sessionId");
    if (sessionId)
      return this.closeSession(sessionId, booleanParam(params.deleteProfile, "deleteProfile"));
    const closed = [];
    for (const id of [...this.sessions.keys()]) closed.push(await this.closeSession(id, false));
    return { closed: true, sessions: closed };
  }

  private async dispatch(
    method: string,
    params: Record<string, unknown>,
    signal?: AbortSignal,
  ): Promise<Record<string, unknown>> {
    await this.rootsReady;
    if (method === "browser.installation") return this.installation();
    if (method === "browser.launch") return this.launchManaged(params);
    if (method === "browser.recover") return this.recover(params);
    if (method === "browser.status") {
      return this.status(
        params.sessionId === undefined ? undefined : identifier(params.sessionId, "sessionId"),
      );
    }
    if (method === "browser.logs") {
      return this.logs({
        ...(params.sessionId === undefined
          ? {}
          : { sessionId: identifier(params.sessionId, "sessionId") }),
        offset: boundedNumber(params.offset, "offset", 0, MAX_LOG_ENTRIES, 0),
        limit: boundedNumber(params.limit, "limit", 1, MAX_LOG_ENTRIES, 100),
      });
    }
    if (method === "browser.close" || method === "browser.kill") {
      return this.close({
        ...params,
        deleteProfile: method === "browser.kill" && params.deleteProfile,
      });
    }

    const session = this.requireSession(
      params.sessionId === undefined ? undefined : identifier(params.sessionId, "sessionId"),
    );
    if (
      session.paused &&
      !["browser.resume", "browser.takeover.open", "browser.takeover.resume"].includes(method)
    ) {
      throw new Error("automation is paused for user takeover");
    }
    const page = this.requirePage(session, params.pageId);
    switch (method) {
      case "browser.navigate":
        return this.navigatePage(page, params);
      case "browser.back":
        return this.navigationResult(page, await page.goBack({ waitUntil: "domcontentloaded" }));
      case "browser.forward":
        return this.navigationResult(page, await page.goForward({ waitUntil: "domcontentloaded" }));
      case "browser.reload":
        return this.navigationResult(page, await page.reload({ waitUntil: "domcontentloaded" }));
      case "browser.stop":
        await page.evaluate(() => window.stop());
        return { stopped: true, url: page.url() };
      case "browser.tab.new":
        return this.newTab(session, params.url);
      case "browser.tab.list":
        return { tabs: this.tabList(session), activePageId: session.activePageId };
      case "browser.tab.select":
        return this.selectTab(session, params.pageId);
      case "browser.tab.close":
        return this.closeTab(session, params.pageId);
      case "browser.screenshot":
        return this.captureScreenshot(session, page, params);
      case "browser.snapshot":
        return this.snapshot(page, params);
      case "browser.performance":
        return this.performance(page);
      case "browser.viewport.set":
        return this.setViewport(page, params);
      case "browser.cookies.get":
        return { cookies: (await session.context.cookies()).slice(0, MAX_COOKIES) };
      case "browser.cookies.set":
        return this.setCookies(session, params);
      case "browser.cookies.clear":
        await session.context.clearCookies();
        return { cleared: true };
      case "browser.storage.get":
        return this.getStorage(page);
      case "browser.storage.set":
        return this.setStorage(page, params);
      case "browser.trace.start":
        return this.traceStart(session, params);
      case "browser.trace.stop":
        return this.traceStop(session, params);
      case "browser.pause":
        return this.pause(session);
      case "browser.resume":
        return this.resume(session);
      case "browser.takeover.open":
        return this.relaunchForTakeover(session, false);
      case "browser.takeover.resume":
        return this.relaunchForTakeover(session, true);
      case "browser.configure":
        return this.configure(session, params);
      case "browser.action":
        return this.performAction(session, page, params, signal);
      default:
        if (method.startsWith("browser.")) {
          return this.performAction(session, page, { ...params, action: method.slice(8) }, signal);
        }
        throw new Error(`unknown method: ${method}`);
    }
  }

  private parseLaunch(params: Record<string, unknown>): LaunchConfig {
    const deviceName =
      params.device === undefined
        ? undefined
        : stringParam(params.device, "device", { required: true, max: 128 });
    if (deviceName && !(deviceName in devices))
      throw new Error(`unknown Playwright device: ${deviceName}`);
    const descriptor = deviceName ? devices[deviceName] : undefined;
    return {
      sessionId: identifier(params.sessionId, "sessionId"),
      profileMode: params.profileMode === "persistent" ? "persistent" : "temporary",
      profileName: identifier(params.profileName, "profileName", "default"),
      headless: booleanParam(params.headless, "headless", true),
      viewport: {
        width: Math.trunc(
          boundedNumber(
            params.width,
            "width",
            320,
            7_680,
            descriptor?.viewport.width ?? DEFAULT_VIEWPORT.width,
          ),
        ),
        height: Math.trunc(
          boundedNumber(
            params.height,
            "height",
            240,
            4_320,
            descriptor?.viewport.height ?? DEFAULT_VIEWPORT.height,
          ),
        ),
      },
      ...(params.userAgent !== undefined
        ? { userAgent: stringParam(params.userAgent, "userAgent", { required: true, max: 1_024 }) }
        : descriptor?.userAgent
          ? { userAgent: descriptor.userAgent }
          : {}),
      ...(deviceName ? { deviceName } : {}),
      recordVideo: booleanParam(params.recordVideo, "recordVideo"),
    };
  }

  private async launchManaged(params: Record<string, unknown>): Promise<Record<string, unknown>> {
    const config = this.parseLaunch(params);
    const existing = this.sessions.get(config.sessionId);
    if (existing) return { alreadyRunning: true, ...this.sessionStatus(existing) };
    const profilePath =
      config.profileMode === "persistent"
        ? join(this.profileRoot, config.profileName)
        : await mkdtemp(join(tmpdir(), `retcon-browser-${config.sessionId}-`));
    const session = await this.createSession(
      config,
      profilePath,
      config.profileMode === "temporary",
    );
    this.sessions.set(config.sessionId, session);
    this.recoverable.set(config.sessionId, config);
    this.emit({ type: "browser.launched", payload: this.sessionStatus(session) });
    return { launched: true, ...this.sessionStatus(session) };
  }

  private async createSession(
    config: LaunchConfig,
    profilePath: string,
    temporaryProfile: boolean,
    restoreUrl?: string,
  ): Promise<Session> {
    const realProfilePath = await ensureRoot(profilePath);
    if (!temporaryProfile && !isWithin(this.profileRoot, realProfilePath)) {
      throw new Error("persistent profile resolves outside the managed profile root");
    }
    const descriptor = config.deviceName ? devices[config.deviceName] : undefined;
    const contextOptions: BrowserContextOptions = {
      ...(descriptor ?? {}),
      viewport: config.viewport,
      ...(config.userAgent ? { userAgent: config.userAgent } : {}),
      acceptDownloads: true,
      ...(config.recordVideo ? { recordVideo: { dir: join(this.artifactRoot, "videos") } } : {}),
    };
    const executablePath = process.env.RETCON_CHROMIUM_PATH;
    const context = await chromium.launchPersistentContext(profilePath, {
      ...contextOptions,
      headless: config.headless,
      ...(executablePath ? { executablePath } : {}),
    });
    const session: Session = {
      id: config.sessionId,
      context,
      pages: new Map(),
      pageIds: new WeakMap(),
      activePageId: "",
      nextPage: 0,
      profilePath,
      temporaryProfile,
      config,
      consoleEntries: [],
      networkEntries: [],
      pageErrors: [],
      expectedClose: false,
      paused: false,
      audit: [],
      tracing: false,
    };
    context.on("page", (page) => this.attachPage(session, page));
    context.on("close", () => {
      if (session.expectedClose) return;
      this.sessions.delete(session.id);
      this.emit({ type: "browser.crashed", payload: { sessionId: session.id } });
      if (session.temporaryProfile) void rm(session.profilePath, { recursive: true, force: true });
    });
    for (const page of context.pages()) this.attachPage(session, page);
    if (session.pages.size === 0) this.attachPage(session, await context.newPage());
    if (restoreUrl && restoreUrl !== "about:blank") await this.activePage(session).goto(restoreUrl);
    return session;
  }

  private attachPage(session: Session, page: Page): string {
    const known = session.pageIds.get(page);
    if (known) return known;
    const pageId = `page-${++session.nextPage}`;
    session.pages.set(pageId, page);
    session.pageIds.set(page, pageId);
    session.activePageId = pageId;
    page.on("close", () => {
      session.pages.delete(pageId);
      if (session.activePageId === pageId)
        session.activePageId = session.pages.keys().next().value ?? "";
      this.emit({ type: "browser.tab.closed", payload: { sessionId: session.id, pageId } });
    });
    page.on("popup", (popup) => this.attachPage(session, popup));
    page.on("console", (message) => {
      const entry = {
        pageId,
        type: message.type(),
        text: boundedText(message.text(), 16_384).text,
        timestamp: new Date().toISOString(),
      };
      pushCapped(session.consoleEntries, entry, MAX_LOG_ENTRIES);
      this.emit({ type: "browser.console", payload: { sessionId: session.id, ...entry } });
    });
    page.on("pageerror", (error) => {
      const entry = {
        pageId,
        message: boundedText(error.message, 16_384).text,
        timestamp: new Date().toISOString(),
      };
      pushCapped(session.pageErrors, entry, MAX_LOG_ENTRIES);
      this.emit({ type: "browser.pageError", payload: { sessionId: session.id, ...entry } });
    });
    page.on("request", (request) => {
      const entry = {
        kind: "request",
        pageId,
        method: request.method(),
        url: boundedText(request.url(), 16_384).text,
        resourceType: request.resourceType(),
        timestamp: new Date().toISOString(),
      };
      pushCapped(session.networkEntries, entry, MAX_LOG_ENTRIES);
    });
    page.on("response", (response) => {
      const entry = {
        kind: "response",
        pageId,
        status: response.status(),
        ok: response.ok(),
        url: boundedText(response.url(), 16_384).text,
        timestamp: new Date().toISOString(),
      };
      pushCapped(session.networkEntries, entry, MAX_LOG_ENTRIES);
      this.emit({ type: "browser.response", payload: { sessionId: session.id, ...entry } });
    });
    this.emit({ type: "browser.tab.opened", payload: { sessionId: session.id, pageId } });
    return pageId;
  }

  private async navigatePage(
    page: Page,
    params: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    const parsed = safeUrl(params.url, this.inputRoots);
    const response = await page.goto(parsed.toString(), {
      waitUntil:
        params.waitUntil === "load" || params.waitUntil === "networkidle"
          ? params.waitUntil
          : "domcontentloaded",
      timeout: timeoutParam(params.timeoutMs),
    });
    return this.navigationResult(page, response);
  }

  private async navigationResult(
    page: Page,
    response: Awaited<ReturnType<Page["goto"]>>,
  ): Promise<Record<string, unknown>> {
    return { url: page.url(), title: await page.title(), status: response?.status() ?? null };
  }

  private async newTab(session: Session, rawUrl: unknown): Promise<Record<string, unknown>> {
    const page = await session.context.newPage();
    const pageId = this.attachPage(session, page);
    if (rawUrl !== undefined) await this.navigatePage(page, { url: rawUrl });
    return { pageId, url: page.url() };
  }

  private selectTab(session: Session, rawPageId: unknown): Record<string, unknown> {
    const pageId = identifier(rawPageId, "pageId");
    const page = session.pages.get(pageId);
    if (!page) throw new Error("tab not found");
    session.activePageId = pageId;
    void page.bringToFront();
    return { selected: true, pageId, url: page.url() };
  }

  private async closeTab(session: Session, rawPageId: unknown): Promise<Record<string, unknown>> {
    const pageId = identifier(rawPageId, "pageId", session.activePageId);
    const page = session.pages.get(pageId);
    if (!page) throw new Error("tab not found");
    await page.close();
    if (session.pages.size === 0) this.attachPage(session, await session.context.newPage());
    return { closed: true, pageId, activePageId: session.activePageId };
  }

  private tabList(session: Session): Record<string, unknown>[] {
    return [...session.pages].map(([pageId, page]) => ({
      pageId,
      url: page.url(),
      active: pageId === session.activePageId,
    }));
  }

  private async captureScreenshot(
    session: Session,
    page: Page,
    params: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    const type = params.type === "jpeg" ? "jpeg" : "png";
    const path = await safeOutputPath(
      this.artifactRoot,
      params.path,
      `screenshots/${session.id}-${Date.now()}.${type === "jpeg" ? "jpg" : "png"}`,
    );
    await page.screenshot({
      path,
      fullPage: booleanParam(params.fullPage, "fullPage"),
      type,
      ...(type === "jpeg"
        ? { quality: Math.trunc(boundedNumber(params.quality, "quality", 1, 100, 80)) }
        : {}),
    });
    const bytes = await enforceFileSize(path, 25 * 1024 * 1024);
    return { path, bytes, fullPage: params.fullPage === true, type };
  }

  private async snapshot(
    page: Page,
    params: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    const includeDom = booleanParam(params.dom, "dom", true);
    const includeAccessibility = booleanParam(params.accessibility, "accessibility", true);
    const dom = includeDom ? boundedText(await page.content()) : undefined;
    const accessibility = includeAccessibility
      ? boundedText(await page.locator("html").ariaSnapshot(), MAX_TEXT_BYTES)
      : undefined;
    return {
      url: page.url(),
      title: await page.title(),
      ...(dom ? { dom: dom.text, domTruncated: dom.truncated } : {}),
      ...(accessibility
        ? { accessibility: accessibility.text, accessibilityTruncated: accessibility.truncated }
        : {}),
    };
  }

  private async performance(page: Page): Promise<Record<string, unknown>> {
    const metrics = await page.evaluate(() => ({
      timeOrigin: performance.timeOrigin,
      navigation: performance.getEntriesByType("navigation").map((entry) => entry.toJSON()),
      paint: performance.getEntriesByType("paint").map((entry) => entry.toJSON()),
      resources: performance.getEntriesByType("resource").length,
    }));
    return boundedJson(metrics) as Record<string, unknown>;
  }

  private async setViewport(
    page: Page,
    params: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    const width = Math.trunc(
      boundedNumber(params.width, "width", 320, 7_680, DEFAULT_VIEWPORT.width),
    );
    const height = Math.trunc(
      boundedNumber(params.height, "height", 240, 4_320, DEFAULT_VIEWPORT.height),
    );
    await page.setViewportSize({ width, height });
    return { width, height };
  }

  private async setCookies(
    session: Session,
    params: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    if (!Array.isArray(params.cookies) || params.cookies.length > MAX_COOKIES) {
      throw new Error(`cookies must be an array with at most ${MAX_COOKIES} entries`);
    }
    const cookies = boundedJson(params.cookies) as Cookie[];
    await session.context.addCookies(cookies);
    return { count: cookies.length };
  }

  private async getStorage(page: Page): Promise<Record<string, unknown>> {
    return boundedJson(
      await page.evaluate(() => ({
        localStorage: Object.fromEntries(Object.entries(localStorage)),
        sessionStorage: Object.fromEntries(Object.entries(sessionStorage)),
      })),
    ) as Record<string, unknown>;
  }

  private async setStorage(
    page: Page,
    params: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    const local =
      params.localStorage === undefined ? {} : objectParam(params.localStorage, "localStorage");
    const session =
      params.sessionStorage === undefined
        ? {}
        : objectParam(params.sessionStorage, "sessionStorage");
    boundedJson({ local, session });
    await page.evaluate(
      ({ local, session }) => {
        for (const [key, value] of Object.entries(local)) localStorage.setItem(key, String(value));
        for (const [key, value] of Object.entries(session))
          sessionStorage.setItem(key, String(value));
      },
      { local, session },
    );
    return { updated: true };
  }

  private async traceStart(
    session: Session,
    params: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    if (session.tracing) return { alreadyTracing: true };
    await session.context.tracing.start({
      screenshots: booleanParam(params.screenshots, "screenshots", true),
      snapshots: booleanParam(params.snapshots, "snapshots", true),
      sources: booleanParam(params.sources, "sources", true),
    });
    session.tracing = true;
    return { tracing: true };
  }

  private async traceStop(
    session: Session,
    params: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    if (!session.tracing) throw new Error("tracing is not active");
    const path = await safeOutputPath(
      this.artifactRoot,
      params.path,
      `traces/${session.id}-${Date.now()}.zip`,
    );
    await session.context.tracing.stop({ path });
    session.tracing = false;
    const bytes = await enforceFileSize(path);
    return { tracing: false, path, bytes };
  }

  private pause(session: Session): Record<string, unknown> {
    if (!session.paused) {
      session.paused = true;
      session.audit.push({ openedAt: new Date().toISOString(), mode: "paused" });
      this.emit({
        type: "browser.takeover.started",
        payload: { sessionId: session.id, mode: "paused" },
      });
    }
    return { paused: true, audit: session.audit };
  }

  private resume(session: Session): Record<string, unknown> {
    session.paused = false;
    const current = session.audit.at(-1);
    if (current && !current.closedAt) current.closedAt = new Date().toISOString();
    this.emit({ type: "browser.takeover.ended", payload: { sessionId: session.id } });
    return { paused: false, audit: session.audit };
  }

  private async relaunchForTakeover(
    session: Session,
    resumeHeadless: boolean,
  ): Promise<Record<string, unknown>> {
    const url = this.activePage(session).url();
    const audit = session.audit;
    if (!resumeHeadless) audit.push({ openedAt: new Date().toISOString(), mode: "headed" });
    else {
      const current = audit.at(-1);
      if (current && !current.closedAt) current.closedAt = new Date().toISOString();
    }
    const config = { ...session.config, headless: resumeHeadless };
    await this.closeContext(session, false, true);
    const replacement = await this.createSession(
      config,
      session.profilePath,
      session.temporaryProfile,
      url,
    );
    replacement.audit = audit;
    replacement.paused = !resumeHeadless;
    this.sessions.set(session.id, replacement);
    this.recoverable.set(session.id, config);
    this.emit({
      type: resumeHeadless ? "browser.takeover.ended" : "browser.takeover.started",
      payload: { sessionId: session.id, mode: resumeHeadless ? "automation" : "headed" },
    });
    return { headed: !resumeHeadless, paused: replacement.paused, url, audit };
  }

  private async configure(
    session: Session,
    params: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    const page = this.activePage(session);
    if (params.width !== undefined || params.height !== undefined)
      await this.setViewport(page, params);
    if (params.userAgent === undefined && params.device === undefined)
      return this.sessionStatus(session);
    const url = page.url();
    const updated = this.parseLaunch({
      ...session.config,
      ...params,
      sessionId: session.id,
      profileMode: session.config.profileMode,
      profileName: session.config.profileName,
      recordVideo: session.config.recordVideo,
    });
    await this.closeContext(session, false, true);
    const replacement = await this.createSession(
      updated,
      session.profilePath,
      session.temporaryProfile,
      url,
    );
    replacement.audit = session.audit;
    this.sessions.set(session.id, replacement);
    this.recoverable.set(session.id, updated);
    return this.sessionStatus(replacement);
  }

  private async performAction(
    session: Session,
    page: Page,
    params: Record<string, unknown>,
    signal?: AbortSignal,
  ): Promise<Record<string, unknown>> {
    const action = stringParam(params.action, "action", { required: true, max: 64 });
    const selector =
      params.selector === undefined
        ? ""
        : stringParam(params.selector, "selector", { required: true, max: 8_192 });
    const timeout = timeoutParam(params.timeoutMs);
    const locator = selector ? page.locator(selector) : undefined;
    const actionOptions = { timeout };
    if (
      [
        "click",
        "fill",
        "type",
        "select",
        "upload",
        "drag",
        "text",
        "download",
        "dialog",
        "popup",
      ].includes(action) &&
      !locator
    ) {
      throw new Error(`${action} requires a selector`);
    }
    switch (action) {
      case "click":
        await locator?.click(actionOptions);
        break;
      case "fill":
        await locator?.fill(
          stringParam(params.value, "value", { max: MAX_TEXT_BYTES }),
          actionOptions,
        );
        break;
      case "type":
        await locator?.pressSequentially(
          stringParam(params.value, "value", { max: MAX_TEXT_BYTES }),
          {
            delay: boundedNumber(params.delay, "delay", 0, 5_000, 0),
            timeout,
          },
        );
        break;
      case "select": {
        const values = Array.isArray(params.values)
          ? params.values.map((value) => stringParam(value, "select value", { max: 4_096 }))
          : [stringParam(params.value, "value", { required: true, max: 4_096 })];
        return { values: await locator?.selectOption(values, actionOptions) };
      }
      case "upload": {
        if (!Array.isArray(params.paths) || params.paths.length === 0 || params.paths.length > 20) {
          throw new Error("paths must contain between 1 and 20 input files");
        }
        const paths = await Promise.all(
          params.paths.map((path) => safeInputPath(this.inputRoots, path)),
        );
        await locator?.setInputFiles(paths, actionOptions);
        return { uploaded: paths.map((path) => basename(path)) };
      }
      case "key":
      case "press":
        if (locator)
          await locator.press(
            stringParam(params.key ?? params.value, "key", { required: true, max: 128 }),
            actionOptions,
          );
        else
          await page.keyboard.press(
            stringParam(params.key ?? params.value, "key", { required: true, max: 128 }),
          );
        break;
      case "scroll":
        await page.mouse.wheel(
          boundedNumber(params.deltaX, "deltaX", -1_000_000, 1_000_000, 0),
          boundedNumber(params.deltaY, "deltaY", -1_000_000, 1_000_000, 0),
        );
        break;
      case "drag":
        await page.dragAndDrop(
          selector,
          stringParam(params.target, "target", { required: true, max: 8_192 }),
          actionOptions,
        );
        break;
      case "wait":
        if (params.url !== undefined)
          await page.waitForURL(safeUrl(params.url, this.inputRoots).toString(), actionOptions);
        else if (selector)
          await locator?.waitFor({
            state: params.state === "hidden" ? "hidden" : "visible",
            timeout,
          });
        else await page.waitForTimeout(boundedNumber(params.ms, "ms", 0, 120_000, 100));
        break;
      case "script": {
        const expression = stringParam(params.script, "script", {
          required: true,
          max: MAX_SCRIPT_BYTES,
        });
        const result = await page.evaluate(expression, boundedJson(params.arg));
        return { value: boundedJson(result) };
      }
      case "text":
        return { text: boundedText((await locator?.innerText(actionOptions)) ?? "").text };
      case "download": {
        const path = await safeOutputPath(
          this.artifactRoot,
          params.path,
          `downloads/${session.id}-${Date.now()}.download`,
        );
        const [download] = await Promise.all([
          page.waitForEvent("download", { timeout }),
          locator?.click(actionOptions),
        ]);
        await download.saveAs(path);
        return {
          path,
          suggestedFilename: download.suggestedFilename(),
          bytes: await enforceFileSize(path),
        };
      }
      case "dialog": {
        const dialogPromise = page.waitForEvent("dialog", { timeout });
        await locator?.click(actionOptions);
        const dialog = await dialogPromise;
        const message = boundedText(dialog.message(), 16_384).text;
        if (booleanParam(params.accept, "accept", true)) {
          await dialog.accept(
            params.promptText === undefined
              ? undefined
              : stringParam(params.promptText, "promptText", { max: 16_384 }),
          );
        } else await dialog.dismiss();
        return { type: dialog.type(), message, accepted: params.accept !== false };
      }
      case "popup": {
        const popupPromise = page.waitForEvent("popup", { timeout });
        await locator?.click(actionOptions);
        const popup = await popupPromise;
        const pageId = this.attachPage(session, popup);
        await popup.waitForLoadState("domcontentloaded", { timeout }).catch(() => undefined);
        return { pageId, url: popup.url() };
      }
      default:
        throw new Error(`unsupported action: ${action}`);
    }
    if (signal?.aborted) throw new Error("operation cancelled");
    return { completed: true };
  }

  private async recover(params: Record<string, unknown>): Promise<Record<string, unknown>> {
    const sessionId = identifier(params.sessionId, "sessionId");
    const existing = this.sessions.get(sessionId);
    if (existing)
      return { recovered: false, alreadyRunning: true, ...this.sessionStatus(existing) };
    const config = this.recoverable.get(sessionId);
    if (!config) throw new Error("no recoverable browser session configuration exists");
    const profilePath =
      config.profileMode === "persistent"
        ? join(this.profileRoot, config.profileName)
        : await mkdtemp(join(tmpdir(), `retcon-browser-${sessionId}-`));
    const session = await this.createSession(
      config,
      profilePath,
      config.profileMode === "temporary",
    );
    this.sessions.set(sessionId, session);
    return { recovered: true, ...this.sessionStatus(session) };
  }

  private async closeSession(
    sessionId: string,
    deleteProfile: boolean,
  ): Promise<Record<string, unknown>> {
    const session = this.sessions.get(sessionId);
    if (!session) return { closed: false, sessionId };
    const videos = [...session.pages.values()]
      .map((page) => page.video())
      .filter((video) => video !== null);
    await this.closeContext(session, deleteProfile);
    const videoPaths: string[] = [];
    for (const video of videos) {
      const path = await video?.path().catch(() => undefined);
      if (path) videoPaths.push(path);
    }
    this.sessions.delete(sessionId);
    this.emit({ type: "browser.closed", payload: { sessionId } });
    return { closed: true, sessionId, videoPaths, audit: session.audit };
  }

  private async closeContext(
    session: Session,
    deleteProfile: boolean,
    preserveTemporary = false,
  ): Promise<void> {
    session.expectedClose = true;
    if (session.tracing) await session.context.tracing.stop().catch(() => undefined);
    await session.context.close().catch(() => undefined);
    if ((session.temporaryProfile && !preserveTemporary) || deleteProfile) {
      await rm(session.profilePath, { recursive: true, force: true });
    }
  }

  private requireSession(sessionId = "default"): Session {
    const session = this.sessions.get(sessionId);
    if (!session) throw new Error(`browser session '${sessionId}' is not running`);
    return session;
  }

  private requirePage(session: Session, rawPageId: unknown): Page {
    if (rawPageId === undefined) return this.activePage(session);
    const page = session.pages.get(identifier(rawPageId, "pageId"));
    if (!page || page.isClosed()) throw new Error("tab not found");
    return page;
  }

  private activePage(session: Session): Page {
    const page = session.pages.get(session.activePageId);
    if (!page || page.isClosed()) throw new Error("browser session has no active tab");
    return page;
  }

  private sessionStatus(session: Session): Record<string, unknown> {
    const page = session.pages.get(session.activePageId);
    return {
      running: true,
      sessionId: session.id,
      pageId: session.activePageId,
      url: page?.url() ?? null,
      pages: session.pages.size,
      profileMode: session.config.profileMode,
      headless: session.config.headless,
      paused: session.paused,
      browserVersion: session.context.browser()?.version() ?? null,
      audit: session.audit,
    };
  }
}
