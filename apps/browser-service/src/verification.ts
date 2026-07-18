import { createHash, randomUUID } from "node:crypto";
import { copyFile, mkdir, mkdtemp, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import pixelmatch from "pixelmatch";
import {
  type BrowserContext,
  type ConsoleMessage,
  type Page,
  type Response,
  chromium,
} from "playwright";
import { PNG } from "pngjs";
import {
  booleanParam,
  boundedJson,
  boundedNumber,
  boundedText,
  enforceFileSize,
  ensureRoot,
  identifier,
  isWithin,
  objectParam,
  safeOutputPath,
  safeUrl,
  stringParam,
} from "./validation.ts";

const MAX_STEPS = 128;
const MAX_VARIANTS = 12;
const MAX_SELECTORS = 64;
const MAX_CAPTURE_ENTRIES = 200;
const MAX_TIMELINE_ENTRIES = 2_048;
const MAX_FINDINGS = 512;
const MAX_ASSERTIONS = 512;
const MAX_ARTIFACTS = 512;
const MAX_VISUAL_COMPARISONS = 256;
const MAX_ACCESSIBILITY_AUDITS = 64;
const MAX_RESULT_BYTES = 2 * 1_048_576;
export const MAX_VERIFICATION_SCREENSHOT_BYTES = 16 * 1_048_576;
const DEFAULT_VIEWPORT = { width: 1280, height: 720 };

export type VerificationStatus = "passed" | "failed" | "cancelled";
export type AssertionStatus = "passed" | "failed";

export interface VerificationViewport {
  name: string;
  width: number;
  height: number;
  deviceScaleFactor: number;
  isMobile: boolean;
}

export interface VisualPolicy {
  updateBaseline: "never" | "missing" | "always";
  pixelThreshold: number;
  maxDiffPixelRatio: number;
  perceptualThreshold: number;
  maskSelectors: string[];
  ignoreSelectors: string[];
}

export type VerificationStep =
  | {
      id: string;
      type: "navigate";
      url?: string;
      waitUntil: "domcontentloaded" | "load" | "networkidle";
      expectedStatus?: number;
      timeoutMs?: number;
    }
  | { id: string; type: "click"; selector: string; timeoutMs?: number }
  | { id: string; type: "fill"; selector: string; value: string; timeoutMs?: number }
  | { id: string; type: "press"; selector?: string; key: string; timeoutMs?: number }
  | {
      id: string;
      type: "wait";
      selector?: string;
      state: "attached" | "detached" | "visible" | "hidden";
      durationMs?: number;
      timeoutMs?: number;
    }
  | {
      id: string;
      type: "assert.text";
      selector?: string;
      expected: string;
      mode: "contains" | "equals" | "matches";
      timeoutMs?: number;
    }
  | {
      id: string;
      type: "assert.element";
      selector: string;
      state: "attached" | "detached" | "visible" | "hidden" | "enabled" | "disabled";
      count?: number;
      timeoutMs?: number;
    }
  | { id: string; type: "assert.status"; expected: number }
  | {
      id: string;
      type: "assert.console";
      level?: string;
      text?: string;
      absent: boolean;
    }
  | {
      id: string;
      type: "screenshot";
      name: string;
      fullPage: boolean;
      compare: boolean;
      maskSelectors: string[];
      ignoreSelectors: string[];
    }
  | { id: string; type: "accessibility"; failOnFindings: boolean };

export interface VerificationDefinition {
  id: string;
  targetUrl: string;
  serverRef: string;
  steps: VerificationStep[];
  variants: VerificationViewport[];
  timeoutMs: number;
  retries: number;
  failOnAccessibility: boolean;
  visual: VisualPolicy;
}

export interface VerificationRequest {
  runId: string;
  artifactDirectory?: string;
  definition: VerificationDefinition;
}

export interface VerificationTimelineEvent {
  sequence: number;
  timestamp: string;
  elapsedMs: number;
  type: string;
  status: "started" | "passed" | "failed" | "info";
  variant?: string;
  stepId?: string;
  attempt?: number;
  details?: Record<string, unknown>;
}

export interface VerificationAssertion {
  sequence: number;
  variant: string;
  stepId: string;
  kind: string;
  status: AssertionStatus;
  expected: unknown;
  actual: unknown;
  message: string;
}

export interface AccessibilityFinding {
  rule: "labels" | "contrast" | "keyboard" | "headings" | "landmarks" | "forms" | "focus";
  severity: "error" | "warning";
  selector: string;
  message: string;
  details?: Record<string, unknown>;
}

export interface VerificationArtifact {
  kind: "screenshot" | "visual-current" | "visual-baseline" | "visual-diff" | "failure";
  path: string;
  bytes: number;
  variant: string;
  stepId: string;
}

export interface VisualComparison {
  variant: string;
  stepId: string;
  name: string;
  status: "passed" | "failed" | "created" | "updated";
  width: number;
  height: number;
  diffPixels: number;
  diffPixelRatio: number;
  perceptualDifference: number;
  thresholds: Pick<VisualPolicy, "pixelThreshold" | "maxDiffPixelRatio" | "perceptualThreshold">;
  baselinePath: string;
  currentPath: string;
  diffPath?: string;
  baselineHistory: string[];
}

interface CaptureState {
  console: Array<{ type: string; text: string; timestamp: string }>;
  network: Array<{
    kind: "request" | "response" | "requestfailed";
    method?: string;
    status?: number;
    ok?: boolean;
    url: string;
    resourceType?: string;
    error?: string;
    timestamp: string;
  }>;
  pageErrors: Array<{ message: string; timestamp: string }>;
}

interface RunState {
  startedAt: number;
  timeline: VerificationTimelineEvent[];
  assertions: VerificationAssertion[];
  artifacts: VerificationArtifact[];
  visuals: VisualComparison[];
  accessibility: Array<{ variant: string; findings: AccessibilityFinding[] }>;
  captures: CaptureState;
}

interface VariantResult {
  name: string;
  status: "passed" | "failed";
  attempts: number;
  durationMs: number;
  error?: string;
}

export interface BrowserVerificationResult {
  runId: string;
  definitionId: string;
  serverRef: string;
  targetUrl: string;
  status: VerificationStatus;
  startedAt: string;
  finishedAt: string;
  durationMs: number;
  variants: VariantResult[];
  timeline: VerificationTimelineEvent[];
  assertions: VerificationAssertion[];
  visualComparisons: VisualComparison[];
  accessibility: Array<{ variant: string; findings: AccessibilityFinding[] }>;
  console: {
    total: number;
    errorCount: number;
    warningCount: number;
    entries: CaptureState["console"];
  };
  network: {
    total: number;
    failedCount: number;
    entries: CaptureState["network"];
  };
  pageErrors: CaptureState["pageErrors"];
  artifacts: VerificationArtifact[];
}

export interface BrowserVerificationRunnerOptions {
  artifactRoot: string;
  inputRoots: string[];
  emit: (event: { type: string; payload: Record<string, unknown> }) => void;
}

class AssertionFailure extends Error {}

export async function enforceVerificationArtifactSize(path: string): Promise<number> {
  return enforceFileSize(path, MAX_VERIFICATION_SCREENSHOT_BYTES);
}

function capPush<T>(values: T[], value: T, maximum: number): void {
  if (values.length < maximum) values.push(value);
}

function recentPush<T>(values: T[], value: T, maximum: number): void {
  values.push(value);
  if (values.length > maximum) values.splice(0, values.length - maximum);
}

function parseSelectors(value: unknown, name: string): string[] {
  if (value === undefined) return [];
  if (!Array.isArray(value) || value.length > MAX_SELECTORS) {
    throw new Error(`${name} must be an array with at most ${MAX_SELECTORS} selectors`);
  }
  return value.map((selector, index) =>
    stringParam(selector, `${name}[${index}]`, { required: true, max: 4_096 }),
  );
}

function optionalTimeout(value: unknown, name: string): number | undefined {
  if (value === undefined) return undefined;
  return Math.trunc(boundedNumber(value, name, 50, 120_000, 30_000));
}

function parseStep(raw: unknown, index: number): VerificationStep {
  const value = objectParam(raw, `steps[${index}]`);
  const type = stringParam(value.type, `steps[${index}].type`, { required: true, max: 32 });
  const id = identifier(value.id, `steps[${index}].id`, `step-${index + 1}`);
  const timeoutMs = optionalTimeout(value.timeoutMs, `${id}.timeoutMs`);
  const selector = (): string =>
    stringParam(value.selector, `${id}.selector`, { required: true, max: 4_096 });
  const withTimeout = <T extends object>(step: T): T & { timeoutMs?: number } =>
    timeoutMs === undefined ? step : { ...step, timeoutMs };

  switch (type) {
    case "navigate": {
      const waitUntil = ["load", "networkidle"].includes(String(value.waitUntil))
        ? (value.waitUntil as "load" | "networkidle")
        : "domcontentloaded";
      const url =
        value.url === undefined
          ? undefined
          : stringParam(value.url, `${id}.url`, { required: true, max: 16_384 });
      const expectedStatus =
        value.expectedStatus === undefined
          ? undefined
          : Math.trunc(boundedNumber(value.expectedStatus, `${id}.expectedStatus`, 100, 599, 200));
      return withTimeout({
        id,
        type,
        waitUntil,
        ...(url ? { url } : {}),
        ...(expectedStatus === undefined ? {} : { expectedStatus }),
      });
    }
    case "click":
      return withTimeout({ id, type, selector: selector() });
    case "fill":
      return withTimeout({
        id,
        type,
        selector: selector(),
        value: stringParam(value.value, `${id}.value`, { max: 65_536 }),
      });
    case "press":
      return withTimeout({
        id,
        type,
        ...(value.selector === undefined ? {} : { selector: selector() }),
        key: stringParam(value.key, `${id}.key`, { required: true, max: 128 }),
      });
    case "wait": {
      const state = ["attached", "detached", "hidden"].includes(String(value.state))
        ? (value.state as "attached" | "detached" | "hidden")
        : "visible";
      const durationMs =
        value.durationMs === undefined
          ? undefined
          : Math.trunc(boundedNumber(value.durationMs, `${id}.durationMs`, 1, 10_000, 100));
      if (value.selector === undefined && durationMs === undefined)
        throw new Error(`${id} requires selector or durationMs`);
      return withTimeout({
        id,
        type,
        state,
        ...(value.selector === undefined ? {} : { selector: selector() }),
        ...(durationMs === undefined ? {} : { durationMs }),
      });
    }
    case "assert.text": {
      const mode = ["equals", "matches"].includes(String(value.mode))
        ? (value.mode as "equals" | "matches")
        : "contains";
      return withTimeout({
        id,
        type,
        ...(value.selector === undefined ? {} : { selector: selector() }),
        expected: stringParam(value.expected, `${id}.expected`, { required: true, max: 65_536 }),
        mode,
      });
    }
    case "assert.element": {
      const state = ["attached", "detached", "hidden", "enabled", "disabled"].includes(
        String(value.state),
      )
        ? (value.state as "attached" | "detached" | "hidden" | "enabled" | "disabled")
        : "visible";
      const count =
        value.count === undefined
          ? undefined
          : Math.trunc(boundedNumber(value.count, `${id}.count`, 0, 10_000, 1));
      return withTimeout({
        id,
        type,
        selector: selector(),
        state,
        ...(count === undefined ? {} : { count }),
      });
    }
    case "assert.status":
      return {
        id,
        type,
        expected: Math.trunc(boundedNumber(value.expected, `${id}.expected`, 100, 599, 200)),
      };
    case "assert.console":
      return {
        id,
        type,
        ...(value.level === undefined
          ? {}
          : { level: stringParam(value.level, `${id}.level`, { required: true, max: 32 }) }),
        ...(value.text === undefined
          ? {}
          : { text: stringParam(value.text, `${id}.text`, { required: true, max: 16_384 }) }),
        absent: booleanParam(value.absent, `${id}.absent`),
      };
    case "screenshot":
      return {
        id,
        type,
        name: identifier(value.name, `${id}.name`, id),
        fullPage: booleanParam(value.fullPage, `${id}.fullPage`, true),
        compare: booleanParam(value.compare, `${id}.compare`, true),
        maskSelectors: parseSelectors(value.maskSelectors, `${id}.maskSelectors`),
        ignoreSelectors: parseSelectors(value.ignoreSelectors, `${id}.ignoreSelectors`),
      };
    case "accessibility":
      return {
        id,
        type,
        failOnFindings: booleanParam(value.failOnFindings, `${id}.failOnFindings`, true),
      };
    default:
      throw new Error(`unsupported verification step type: ${type}`);
  }
}

function parseViewport(raw: unknown, index: number): VerificationViewport {
  const value = objectParam(raw, `variants[${index}]`);
  return {
    name: identifier(value.name, `variants[${index}].name`, `viewport-${index + 1}`),
    width: Math.trunc(
      boundedNumber(value.width, `variants[${index}].width`, 320, 7_680, DEFAULT_VIEWPORT.width),
    ),
    height: Math.trunc(
      boundedNumber(value.height, `variants[${index}].height`, 240, 4_320, DEFAULT_VIEWPORT.height),
    ),
    deviceScaleFactor: boundedNumber(
      value.deviceScaleFactor,
      `variants[${index}].deviceScaleFactor`,
      1,
      3,
      1,
    ),
    isMobile: booleanParam(value.isMobile, `variants[${index}].isMobile`),
  };
}

export function parseVerificationRequest(raw: unknown, inputRoots: string[]): VerificationRequest {
  const request = objectParam(raw, "verification request");
  const definitionRaw = objectParam(request.definition, "definition");
  if (!Array.isArray(definitionRaw.steps) || definitionRaw.steps.length === 0) {
    throw new Error("definition.steps must contain at least one step");
  }
  if (definitionRaw.steps.length > MAX_STEPS)
    throw new Error(`definition.steps exceeds ${MAX_STEPS} steps`);
  const rawVariants = definitionRaw.variants;
  if (rawVariants !== undefined && (!Array.isArray(rawVariants) || rawVariants.length === 0)) {
    throw new Error("definition.variants must be a non-empty array");
  }
  if (Array.isArray(rawVariants) && rawVariants.length > MAX_VARIANTS) {
    throw new Error(`definition.variants exceeds ${MAX_VARIANTS} variants`);
  }
  const visualRaw =
    definitionRaw.visual === undefined
      ? {}
      : objectParam(definitionRaw.visual, "definition.visual");
  const updateBaseline = ["never", "always"].includes(String(visualRaw.updateBaseline))
    ? (visualRaw.updateBaseline as "never" | "always")
    : "missing";
  const targetUrl = safeUrl(definitionRaw.targetUrl, inputRoots).toString();
  const definition: VerificationDefinition = {
    id: identifier(
      stringParam(definitionRaw.id, "definition.id", { required: true, max: 64 }),
      "definition.id",
    ),
    targetUrl,
    serverRef: identifier(
      stringParam(definitionRaw.serverRef, "definition.serverRef", { required: true, max: 64 }),
      "definition.serverRef",
    ),
    steps: definitionRaw.steps.map(parseStep),
    variants: Array.isArray(rawVariants)
      ? rawVariants.map(parseViewport)
      : [{ name: "desktop", ...DEFAULT_VIEWPORT, deviceScaleFactor: 1, isMobile: false }],
    timeoutMs: Math.trunc(
      boundedNumber(definitionRaw.timeoutMs, "definition.timeoutMs", 500, 120_000, 30_000),
    ),
    retries: Math.trunc(boundedNumber(definitionRaw.retries, "definition.retries", 0, 3, 0)),
    failOnAccessibility: booleanParam(
      definitionRaw.failOnAccessibility,
      "definition.failOnAccessibility",
      true,
    ),
    visual: {
      updateBaseline,
      pixelThreshold: boundedNumber(visualRaw.pixelThreshold, "visual.pixelThreshold", 0, 1, 0.1),
      maxDiffPixelRatio: boundedNumber(
        visualRaw.maxDiffPixelRatio,
        "visual.maxDiffPixelRatio",
        0,
        1,
        0.001,
      ),
      perceptualThreshold: boundedNumber(
        visualRaw.perceptualThreshold,
        "visual.perceptualThreshold",
        0,
        1,
        0.01,
      ),
      maskSelectors: parseSelectors(visualRaw.maskSelectors, "visual.maskSelectors"),
      ignoreSelectors: parseSelectors(visualRaw.ignoreSelectors, "visual.ignoreSelectors"),
    },
  };
  const names = new Set(definition.variants.map((variant) => variant.name));
  if (names.size !== definition.variants.length) throw new Error("variant names must be unique");
  return {
    runId: identifier(request.runId, "runId", randomUUID()),
    ...(request.artifactDirectory === undefined
      ? {}
      : {
          artifactDirectory: stringParam(request.artifactDirectory, "artifactDirectory", {
            required: true,
            max: 4_096,
          }),
        }),
    definition,
  };
}

function redact(value: string): string {
  return boundedText(
    value
      .replace(/\bBearer\s+[A-Za-z0-9._~+/=-]+/gi, "Bearer [REDACTED]")
      .replace(/\b(api[_-]?key|token|authorization|password)=([^&\s]+)/gi, "$1=[REDACTED]")
      .replace(/\b[A-Fa-f0-9]{40,}\b/g, "[REDACTED]"),
    2_048,
  ).text;
}

function sanitize(value: unknown, depth = 0): unknown {
  if (depth > 8) return "[TRUNCATED]";
  if (typeof value === "string") return redact(value);
  if (Array.isArray(value)) return value.slice(0, 50).map((entry) => sanitize(entry, depth + 1));
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(
    Object.entries(value as Record<string, unknown>)
      .slice(0, 50)
      .map(([key, entry]) => [
        key,
        /authorization|password|secret|token|api.?key/i.test(key)
          ? "[REDACTED]"
          : sanitize(entry, depth + 1),
      ]),
  );
}

function capturePage(page: Page, captures: CaptureState): void {
  page.on("console", (message: ConsoleMessage) => {
    recentPush(
      captures.console,
      { type: message.type(), text: redact(message.text()), timestamp: new Date().toISOString() },
      MAX_CAPTURE_ENTRIES,
    );
  });
  page.on("pageerror", (error) => {
    recentPush(
      captures.pageErrors,
      { message: redact(error.message), timestamp: new Date().toISOString() },
      MAX_CAPTURE_ENTRIES,
    );
  });
  page.on("request", (request) => {
    recentPush(
      captures.network,
      {
        kind: "request",
        method: request.method(),
        url: redact(request.url()),
        resourceType: request.resourceType(),
        timestamp: new Date().toISOString(),
      },
      MAX_CAPTURE_ENTRIES,
    );
  });
  page.on("response", (response) => {
    recentPush(
      captures.network,
      {
        kind: "response",
        status: response.status(),
        ok: response.ok(),
        url: redact(response.url()),
        timestamp: new Date().toISOString(),
      },
      MAX_CAPTURE_ENTRIES,
    );
  });
  page.on("requestfailed", (request) => {
    recentPush(
      captures.network,
      {
        kind: "requestfailed",
        method: request.method(),
        url: redact(request.url()),
        resourceType: request.resourceType(),
        error: redact(request.failure()?.errorText ?? "request failed"),
        timestamp: new Date().toISOString(),
      },
      MAX_CAPTURE_ENTRIES,
    );
  });
}

export async function auditAccessibility(page: Page): Promise<AccessibilityFinding[]> {
  const findings = await page.evaluate(() => {
    const output: AccessibilityFinding[] = [];
    const selectorFor = (element: Element): string => {
      if (element.id) return `#${CSS.escape(element.id)}`;
      const tag = element.tagName.toLowerCase();
      const name = element.getAttribute("name");
      if (name) return `${tag}[name=${JSON.stringify(name)}]`;
      const parent = element.parentElement;
      if (!parent) return tag;
      return `${selectorFor(parent)} > ${tag}:nth-child(${[...parent.children].indexOf(element) + 1})`;
    };
    const visible = (element: Element): boolean => {
      const style = getComputedStyle(element);
      const box = element.getBoundingClientRect();
      return (
        style.display !== "none" && style.visibility !== "hidden" && box.width > 0 && box.height > 0
      );
    };
    const push = (finding: AccessibilityFinding): void => {
      if (output.length < 512) output.push(finding);
    };
    const nameOf = (element: Element): string => {
      const labelledBy = element.getAttribute("aria-labelledby");
      const labelled = labelledBy
        ?.split(/\s+/)
        .map((id) => document.getElementById(id)?.textContent ?? "")
        .join(" ")
        .trim();
      if (labelled) return labelled;
      const aria = element.getAttribute("aria-label")?.trim();
      if (aria) return aria;
      if (element instanceof HTMLInputElement && element.id) {
        const label = document.querySelector(`label[for=${JSON.stringify(element.id)}]`);
        if (label?.textContent?.trim()) return label.textContent.trim();
      }
      const wrapping = element.closest("label")?.textContent?.trim();
      if (wrapping) return wrapping;
      return "";
    };
    const parseRgb = (value: string): [number, number, number] | undefined => {
      const match = value.match(/rgba?\((\d+(?:\.\d+)?)[, ]+(\d+(?:\.\d+)?)[, ]+(\d+(?:\.\d+)?)/);
      return match ? [Number(match[1]), Number(match[2]), Number(match[3])] : undefined;
    };
    const luminance = ([red, green, blue]: [number, number, number]): number => {
      const values = [red, green, blue].map((channel) => {
        const normalized = channel / 255;
        return normalized <= 0.03928 ? normalized / 12.92 : ((normalized + 0.055) / 1.055) ** 2.4;
      });
      return 0.2126 * (values[0] ?? 0) + 0.7152 * (values[1] ?? 0) + 0.0722 * (values[2] ?? 0);
    };

    for (const control of document.querySelectorAll("input:not([type=hidden]), select, textarea")) {
      if (!visible(control) || nameOf(control)) continue;
      push({
        rule: "labels",
        severity: "error",
        selector: selectorFor(control),
        message: "form control has no accessible label",
      });
    }
    for (const element of document.querySelectorAll("button, a[href], [role=button], [tabindex]")) {
      if (!visible(element)) continue;
      const html = element as HTMLElement;
      if (html.tabIndex < 0 && element.getAttribute("aria-disabled") !== "true") {
        push({
          rule: "keyboard",
          severity: "error",
          selector: selectorFor(element),
          message: "interactive element is not keyboard focusable",
        });
      }
    }
    const headings = [...document.querySelectorAll("h1,h2,h3,h4,h5,h6")].filter(visible);
    if (!headings.some((heading) => heading.tagName === "H1")) {
      push({
        rule: "headings",
        severity: "warning",
        selector: "html",
        message: "page has no visible h1",
      });
    }
    let previous = 0;
    for (const heading of headings) {
      const level = Number(heading.tagName.slice(1));
      if (previous > 0 && level > previous + 1) {
        push({
          rule: "headings",
          severity: "warning",
          selector: selectorFor(heading),
          message: `heading level skips from h${previous} to h${level}`,
        });
      }
      previous = level;
    }
    const mains = [...document.querySelectorAll("main, [role=main]")].filter(visible);
    if (mains.length !== 1) {
      push({
        rule: "landmarks",
        severity: "error",
        selector: "html",
        message:
          mains.length === 0 ? "page has no main landmark" : "page has multiple main landmarks",
      });
    }
    for (const form of document.querySelectorAll("form")) {
      if (!visible(form)) continue;
      if (!form.getAttribute("aria-label") && !form.getAttribute("aria-labelledby")) {
        push({
          rule: "forms",
          severity: "warning",
          selector: selectorFor(form),
          message: "form has no accessible name",
        });
      }
    }
    for (const element of document.querySelectorAll("p,span,a,button,label,h1,h2,h3,h4,h5,h6")) {
      if (!visible(element) || !element.textContent?.trim()) continue;
      const style = getComputedStyle(element);
      const foreground = parseRgb(style.color);
      let backgroundElement: Element | null = element;
      let background: [number, number, number] | undefined;
      while (backgroundElement && !background) {
        const candidate = parseRgb(getComputedStyle(backgroundElement).backgroundColor);
        if (candidate && getComputedStyle(backgroundElement).backgroundColor !== "rgba(0, 0, 0, 0)")
          background = candidate;
        backgroundElement = backgroundElement.parentElement;
      }
      if (!foreground || !background) continue;
      const first = luminance(foreground);
      const second = luminance(background);
      const ratio = (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
      const large =
        Number.parseFloat(style.fontSize) >= 24 ||
        (Number.parseFloat(style.fontSize) >= 18.66 &&
          Number.parseInt(style.fontWeight, 10) >= 700);
      const required = large ? 3 : 4.5;
      if (ratio < required) {
        push({
          rule: "contrast",
          severity: "error",
          selector: selectorFor(element),
          message: `text contrast ${ratio.toFixed(2)} is below ${required.toFixed(1)}`,
          details: { ratio: Number(ratio.toFixed(3)), required },
        });
      }
    }
    return output;
  });

  const focusCandidates = page.locator(
    "a[href], button, input:not([type=hidden]), select, textarea, [tabindex]:not([tabindex='-1'])",
  );
  const count = Math.min(await focusCandidates.count(), 50);
  if (count > 1) {
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    let previous = "";
    let unchanged = 0;
    for (let index = 0; index < Math.min(count + 2, 25); index += 1) {
      await page.keyboard.press("Tab");
      const current = await page.evaluate(() => {
        const element = document.activeElement;
        if (!(element instanceof HTMLElement)) return "none";
        if (element.id) return `#${CSS.escape(element.id)}`;
        return `${element.tagName.toLowerCase()}${element.getAttribute("name") ? `[name=${JSON.stringify(element.getAttribute("name"))}]` : ""}`;
      });
      unchanged =
        current === previous && !["body", "html", "none"].includes(current) ? unchanged + 1 : 0;
      previous = current;
      if (unchanged >= 2) {
        findings.push({
          rule: "keyboard",
          severity: "error",
          selector: current,
          message: "Tab navigation appears trapped on one element",
          details: { heuristic: "three consecutive Tab presses retained focus" },
        });
        break;
      }
    }
  }
  for (let index = 0; index < count && findings.length < MAX_FINDINGS; index += 1) {
    const candidate = focusCandidates.nth(index);
    if (!(await candidate.isVisible())) continue;
    const focus = await candidate.evaluate((element) => {
      const html = element as HTMLElement;
      const selector = html.id
        ? `#${CSS.escape(html.id)}`
        : `${html.tagName.toLowerCase()}${html.getAttribute("name") ? `[name=${JSON.stringify(html.getAttribute("name"))}]` : ""}`;
      html.blur();
      const before = getComputedStyle(html);
      const normal = {
        outline: before.outline,
        boxShadow: before.boxShadow,
        borderColor: before.borderColor,
      };
      html.focus();
      const after = getComputedStyle(html);
      return {
        selector,
        changed:
          normal.outline !== after.outline ||
          normal.boxShadow !== after.boxShadow ||
          normal.borderColor !== after.borderColor,
        outline: after.outline,
        boxShadow: after.boxShadow,
      };
    });
    if (
      !focus.changed &&
      (focus.outline === "none" || focus.outline.startsWith("0px")) &&
      focus.boxShadow === "none"
    ) {
      findings.push({
        rule: "focus",
        severity: "error",
        selector: focus.selector,
        message: "keyboard focus has no visible indicator",
      });
    }
  }
  return findings.slice(0, MAX_FINDINGS);
}

function rectanglePixels(
  png: PNG,
  rectangles: Array<{ x: number; y: number; width: number; height: number }>,
  source: PNG,
): void {
  for (const rectangle of rectangles) {
    const left = Math.max(0, Math.floor(rectangle.x));
    const top = Math.max(0, Math.floor(rectangle.y));
    const right = Math.min(png.width, Math.ceil(rectangle.x + rectangle.width));
    const bottom = Math.min(png.height, Math.ceil(rectangle.y + rectangle.height));
    for (let y = top; y < bottom; y += 1) {
      for (let x = left; x < right; x += 1) {
        const offset = (y * png.width + x) * 4;
        for (let channel = 0; channel < 4; channel += 1) {
          png.data[offset + channel] = source.data[offset + channel] ?? 0;
        }
      }
    }
  }
}

function perceptualDifference(first: PNG, second: PNG): number {
  let total = 0;
  const pixels = first.width * first.height;
  for (let offset = 0; offset < first.data.length; offset += 4) {
    const firstLuma =
      0.2126 * (first.data[offset] ?? 0) +
      0.7152 * (first.data[offset + 1] ?? 0) +
      0.0722 * (first.data[offset + 2] ?? 0);
    const secondLuma =
      0.2126 * (second.data[offset] ?? 0) +
      0.7152 * (second.data[offset + 1] ?? 0) +
      0.0722 * (second.data[offset + 2] ?? 0);
    total += Math.abs(firstLuma - secondLuma) / 255;
  }
  return pixels === 0 ? 0 : total / pixels;
}

async function ignoredRectangles(
  page: Page,
  selectors: string[],
): Promise<Array<{ x: number; y: number; width: number; height: number }>> {
  const rectangles: Array<{ x: number; y: number; width: number; height: number }> = [];
  for (const selector of selectors) {
    const locator = page.locator(selector);
    const count = Math.min(await locator.count(), 100);
    for (let index = 0; index < count; index += 1) {
      const rectangle = await locator.nth(index).boundingBox();
      if (rectangle) rectangles.push(rectangle);
    }
  }
  return rectangles;
}

async function historyPaths(directory: string): Promise<string[]> {
  try {
    const entries = (await readdir(directory)).filter((entry) => entry.endsWith(".png")).sort();
    const expired = entries.slice(0, Math.max(0, entries.length - 20));
    await Promise.all(expired.map((entry) => rm(join(directory, entry), { force: true })));
    return entries.slice(-20).map((entry) => join(directory, entry));
  } catch {
    return [];
  }
}

async function fileExists(path: string): Promise<boolean> {
  try {
    return (await stat(path)).isFile();
  } catch {
    return false;
  }
}

export class BrowserVerificationRunner {
  private readonly artifactRoot: string;
  private readonly inputRoots: string[];
  private readonly emit: BrowserVerificationRunnerOptions["emit"];

  constructor(options: BrowserVerificationRunnerOptions) {
    this.artifactRoot = resolve(options.artifactRoot);
    this.inputRoots = options.inputRoots.map((root) => resolve(root));
    this.emit = options.emit;
  }

  async run(raw: unknown, signal?: AbortSignal): Promise<BrowserVerificationResult> {
    const request = parseVerificationRequest(raw, this.inputRoots);
    await ensureRoot(this.artifactRoot);
    const requestedDirectory = request.artifactDirectory ?? `verifications/${request.runId}`;
    const directory = await safeOutputPath(
      this.artifactRoot,
      requestedDirectory,
      `verifications/${request.runId}`,
    );
    await mkdir(directory, { recursive: true });
    const realDirectory = await ensureRoot(directory);
    if (!isWithin(this.artifactRoot, realDirectory))
      throw new Error("verification artifact directory escapes the managed root");
    const baselineRoot = await ensureRoot(join(this.artifactRoot, "visual-baselines"));
    const state: RunState = {
      startedAt: Date.now(),
      timeline: [],
      assertions: [],
      artifacts: [],
      visuals: [],
      accessibility: [],
      captures: { console: [], network: [], pageErrors: [] },
    };
    const variants: VariantResult[] = [];
    const startedAt = new Date(state.startedAt).toISOString();
    this.event(state, "run.started", "started", {
      runId: request.runId,
      definitionId: request.definition.id,
    });
    let cancelled = false;
    try {
      for (const variant of request.definition.variants) {
        if (signal?.aborted) throw new Error("operation cancelled");
        variants.push(
          await this.runVariant(request, variant, realDirectory, baselineRoot, state, signal),
        );
      }
    } catch (error) {
      cancelled = signal?.aborted === true || String(error).includes("cancelled");
      if (!cancelled) {
        this.event(state, "run.error", "failed", { error: redact(String(error)) });
        if (variants.length === 0) {
          variants.push({
            name: "runner",
            status: "failed",
            attempts: 1,
            durationMs: Date.now() - state.startedAt,
            error: redact(String(error)),
          });
        }
      }
    }
    const failed = variants.some((variant) => variant.status === "failed");
    const status: VerificationStatus = cancelled ? "cancelled" : failed ? "failed" : "passed";
    this.event(state, "run.finished", status === "passed" ? "passed" : "failed", { status });
    const finishedAt = new Date().toISOString();
    const result: BrowserVerificationResult = {
      runId: request.runId,
      definitionId: request.definition.id,
      serverRef: request.definition.serverRef,
      targetUrl: redact(request.definition.targetUrl),
      status,
      startedAt,
      finishedAt,
      durationMs: Date.now() - state.startedAt,
      variants,
      timeline: state.timeline,
      assertions: state.assertions,
      visualComparisons: state.visuals,
      accessibility: state.accessibility,
      console: {
        total: state.captures.console.length,
        errorCount: state.captures.console.filter((entry) => entry.type === "error").length,
        warningCount: state.captures.console.filter((entry) => entry.type === "warning").length,
        entries: state.captures.console,
      },
      network: {
        total: state.captures.network.length,
        failedCount: state.captures.network.filter(
          (entry) =>
            entry.kind === "requestfailed" || (entry.status !== undefined && entry.status >= 400),
        ).length,
        entries: state.captures.network,
      },
      pageErrors: state.captures.pageErrors,
      artifacts: state.artifacts,
    };
    const bounded = boundedJson(result, MAX_RESULT_BYTES) as BrowserVerificationResult;
    this.emit({
      type: "browser.verification.completed",
      payload: { runId: bounded.runId, status: bounded.status, durationMs: bounded.durationMs },
    });
    return bounded;
  }

  private event(
    state: RunState,
    type: string,
    status: VerificationTimelineEvent["status"],
    details?: Record<string, unknown>,
    context: Pick<VerificationTimelineEvent, "variant" | "stepId" | "attempt"> = {},
  ): void {
    if (state.timeline.length >= MAX_TIMELINE_ENTRIES) return;
    const event: VerificationTimelineEvent = {
      sequence: state.timeline.length + 1,
      timestamp: new Date().toISOString(),
      elapsedMs: Date.now() - state.startedAt,
      type,
      status,
      ...context,
      ...(details
        ? { details: boundedJson(sanitize(details), 32_768) as Record<string, unknown> }
        : {}),
    };
    state.timeline.push(event);
    this.emit({
      type: `browser.verification.${type}`,
      payload: event as unknown as Record<string, unknown>,
    });
  }

  private async runVariant(
    request: VerificationRequest,
    variant: VerificationViewport,
    directory: string,
    baselineRoot: string,
    state: RunState,
    signal?: AbortSignal,
  ): Promise<VariantResult> {
    const started = Date.now();
    let lastError = "";
    for (let attempt = 1; attempt <= request.definition.retries + 1; attempt += 1) {
      let context: BrowserContext | undefined;
      let profilePath: string | undefined;
      let abortContext: (() => void) | undefined;
      const attemptCaptures: CaptureState = { console: [], network: [], pageErrors: [] };
      this.event(
        state,
        "variant.started",
        "started",
        { viewport: variant },
        { variant: variant.name, attempt },
      );
      try {
        profilePath = await mkdtemp(join(tmpdir(), `retcon-verification-${request.runId}-`));
        const executablePath = process.env.RETCON_CHROMIUM_PATH;
        context = await chromium.launchPersistentContext(profilePath, {
          headless: true,
          viewport: { width: variant.width, height: variant.height },
          deviceScaleFactor: variant.deviceScaleFactor,
          isMobile: variant.isMobile,
          ...(executablePath ? { executablePath } : {}),
        });
        abortContext = () => {
          void context?.close().catch(() => undefined);
        };
        signal?.addEventListener("abort", abortContext, { once: true });
        if (signal?.aborted) throw new Error("operation cancelled");
        const page = context.pages()[0] ?? (await context.newPage());
        capturePage(page, attemptCaptures);
        let response = await page.goto(request.definition.targetUrl, {
          waitUntil: "domcontentloaded",
          timeout: request.definition.timeoutMs,
        });
        this.event(
          state,
          "navigation.completed",
          "passed",
          { url: page.url(), status: response?.status() ?? null },
          { variant: variant.name, attempt },
        );
        for (const step of request.definition.steps) {
          if (signal?.aborted) throw new Error("operation cancelled");
          response = await this.executeStep(
            page,
            response,
            step,
            request,
            variant,
            directory,
            baselineRoot,
            state,
            attempt,
            attemptCaptures,
            signal,
          );
        }
        const findings = await auditAccessibility(page);
        capPush(state.accessibility, { variant: variant.name, findings }, MAX_ACCESSIBILITY_AUDITS);
        this.event(
          state,
          "accessibility.completed",
          findings.some((finding) => finding.severity === "error") ? "failed" : "passed",
          {
            findings: findings.length,
            errors: findings.filter((finding) => finding.severity === "error").length,
          },
          { variant: variant.name, attempt },
        );
        if (
          request.definition.failOnAccessibility &&
          findings.some((finding) => finding.severity === "error")
        ) {
          throw new AssertionFailure("accessibility audit produced blocking findings");
        }
        this.event(state, "variant.finished", "passed", undefined, {
          variant: variant.name,
          attempt,
        });
        return {
          name: variant.name,
          status: "passed",
          attempts: attempt,
          durationMs: Date.now() - started,
        };
      } catch (error) {
        lastError = redact(error instanceof Error ? error.message : String(error));
        this.event(
          state,
          "variant.attempt.failed",
          "failed",
          { error: lastError },
          { variant: variant.name, attempt },
        );
        const pages = context?.pages() ?? [];
        const page = pages[0];
        if (page && !page.isClosed()) {
          const path = await safeOutputPath(
            this.artifactRoot,
            join(directory, variant.name, `attempt-${attempt}-failure.png`),
            join(directory, variant.name, `attempt-${attempt}-failure.png`),
          );
          await page.screenshot({ path, fullPage: true }).catch(() => undefined);
          if (await fileExists(path)) {
            capPush(
              state.artifacts,
              {
                kind: "failure",
                path,
                bytes: await enforceVerificationArtifactSize(path),
                variant: variant.name,
                stepId: "failure",
              },
              MAX_ARTIFACTS,
            );
          }
        }
        if (signal?.aborted) throw error;
      } finally {
        if (abortContext) signal?.removeEventListener("abort", abortContext);
        await context?.close().catch(() => undefined);
        if (profilePath) await rm(profilePath, { recursive: true, force: true });
        for (const entry of attemptCaptures.console)
          recentPush(state.captures.console, entry, MAX_CAPTURE_ENTRIES);
        for (const entry of attemptCaptures.network)
          recentPush(state.captures.network, entry, MAX_CAPTURE_ENTRIES);
        for (const entry of attemptCaptures.pageErrors)
          recentPush(state.captures.pageErrors, entry, MAX_CAPTURE_ENTRIES);
      }
    }
    this.event(
      state,
      "variant.finished",
      "failed",
      { error: lastError },
      { variant: variant.name, attempt: request.definition.retries + 1 },
    );
    return {
      name: variant.name,
      status: "failed",
      attempts: request.definition.retries + 1,
      durationMs: Date.now() - started,
      error: lastError,
    };
  }

  private assertion(
    state: RunState,
    variant: string,
    step: VerificationStep,
    kind: string,
    passed: boolean,
    expected: unknown,
    actual: unknown,
    message: string,
  ): void {
    const assertion: VerificationAssertion = {
      sequence: state.assertions.length + 1,
      variant,
      stepId: step.id,
      kind,
      status: passed ? "passed" : "failed",
      expected: boundedJson(sanitize(expected), 4_096),
      actual: boundedJson(sanitize(actual), 4_096),
      message: boundedText(message, 4_096).text,
    };
    capPush(state.assertions, assertion, MAX_ASSERTIONS);
    if (!passed) throw new AssertionFailure(message);
  }

  private async executeStep(
    page: Page,
    previousResponse: Response | null,
    step: VerificationStep,
    request: VerificationRequest,
    variant: VerificationViewport,
    directory: string,
    baselineRoot: string,
    state: RunState,
    attempt: number,
    captures: CaptureState,
    signal?: AbortSignal,
  ): Promise<Response | null> {
    this.event(
      state,
      "step.started",
      "started",
      { type: step.type },
      { variant: variant.name, stepId: step.id, attempt },
    );
    const timeout =
      "timeoutMs" in step && step.timeoutMs ? step.timeoutMs : request.definition.timeoutMs;
    let response = previousResponse;
    switch (step.type) {
      case "navigate": {
        const url = safeUrl(step.url ?? request.definition.targetUrl, this.inputRoots).toString();
        response = await page.goto(url, { waitUntil: step.waitUntil, timeout });
        if (step.expectedStatus !== undefined) {
          this.assertion(
            state,
            variant.name,
            step,
            "status",
            response?.status() === step.expectedStatus,
            step.expectedStatus,
            response?.status() ?? null,
            `expected HTTP ${step.expectedStatus}, received ${response?.status() ?? "no response"}`,
          );
        }
        break;
      }
      case "click":
        await page.locator(step.selector).click({ timeout });
        break;
      case "fill":
        await page.locator(step.selector).fill(step.value, { timeout });
        break;
      case "press":
        if (step.selector) await page.locator(step.selector).press(step.key, { timeout });
        else await page.keyboard.press(step.key);
        break;
      case "wait":
        if (step.selector)
          await page.locator(step.selector).waitFor({ state: step.state, timeout });
        if (step.durationMs)
          await new Promise((resolvePromise) => setTimeout(resolvePromise, step.durationMs));
        break;
      case "assert.text": {
        const actual = boundedText(
          step.selector
            ? ((await page.locator(step.selector).textContent({ timeout })) ?? "")
            : await page.locator("body").innerText({ timeout }),
          65_536,
        ).text;
        let passed = actual.includes(step.expected);
        if (step.mode === "equals") passed = actual.trim() === step.expected;
        if (step.mode === "matches") {
          try {
            passed = new RegExp(step.expected, "u").test(actual);
          } catch {
            throw new Error(`${step.id}.expected must be a valid regular expression`);
          }
        }
        this.assertion(
          state,
          variant.name,
          step,
          "text",
          passed,
          step.expected,
          actual,
          `text assertion failed for ${step.selector ?? "body"}`,
        );
        break;
      }
      case "assert.element": {
        const locator = page.locator(step.selector);
        const count = await locator.count();
        let actual: unknown = count;
        let passed = step.count === undefined || count === step.count;
        if (step.count === undefined) {
          if (step.state === "attached") passed = count > 0;
          else if (step.state === "detached") passed = count === 0;
          else {
            const first = locator.first();
            if (step.state === "visible") actual = passed = count > 0 && (await first.isVisible());
            if (step.state === "hidden")
              actual = passed = count === 0 || !(await first.isVisible());
            if (step.state === "enabled") actual = passed = count > 0 && (await first.isEnabled());
            if (step.state === "disabled")
              actual = passed = count > 0 && !(await first.isEnabled());
          }
        }
        this.assertion(
          state,
          variant.name,
          step,
          "element",
          passed,
          step.count ?? step.state,
          actual,
          `element assertion failed for ${step.selector}`,
        );
        break;
      }
      case "assert.status":
        this.assertion(
          state,
          variant.name,
          step,
          "status",
          previousResponse?.status() === step.expected,
          step.expected,
          previousResponse?.status() ?? null,
          `expected HTTP ${step.expected}, received ${previousResponse?.status() ?? "no response"}`,
        );
        break;
      case "assert.console": {
        const matching = captures.console.filter(
          (entry) =>
            (!step.level || entry.type === step.level) &&
            (!step.text || entry.text.includes(step.text)),
        );
        const passed = step.absent ? matching.length === 0 : matching.length > 0;
        this.assertion(
          state,
          variant.name,
          step,
          "console",
          passed,
          step.absent ? "absent" : "present",
          matching.slice(0, 20),
          "console assertion failed",
        );
        break;
      }
      case "screenshot": {
        const visual = await this.captureVisual(
          page,
          step,
          request,
          variant,
          directory,
          baselineRoot,
          state,
        );
        if (step.compare) {
          this.assertion(
            state,
            variant.name,
            step,
            "visual",
            visual.status !== "failed",
            visual.thresholds,
            {
              diffPixelRatio: visual.diffPixelRatio,
              perceptualDifference: visual.perceptualDifference,
            },
            "visual comparison exceeded configured thresholds",
          );
        }
        break;
      }
      case "accessibility": {
        const findings = await auditAccessibility(page);
        capPush(state.accessibility, { variant: variant.name, findings }, MAX_ACCESSIBILITY_AUDITS);
        this.assertion(
          state,
          variant.name,
          step,
          "accessibility",
          !step.failOnFindings || !findings.some((finding) => finding.severity === "error"),
          "no blocking findings",
          findings,
          "accessibility audit produced blocking findings",
        );
        break;
      }
    }
    if (signal?.aborted) throw new Error("operation cancelled");
    this.event(
      state,
      "step.finished",
      "passed",
      { type: step.type },
      { variant: variant.name, stepId: step.id, attempt },
    );
    return response;
  }

  private async captureVisual(
    page: Page,
    step: Extract<VerificationStep, { type: "screenshot" }>,
    request: VerificationRequest,
    variant: VerificationViewport,
    directory: string,
    baselineRoot: string,
    state: RunState,
  ): Promise<VisualComparison> {
    const variantDirectory = await ensureRoot(join(directory, variant.name));
    const currentPath = await safeOutputPath(
      this.artifactRoot,
      join(variantDirectory, `${step.name}-current.png`),
      join(variantDirectory, `${step.name}-current.png`),
    );
    const masks = [...request.definition.visual.maskSelectors, ...step.maskSelectors].map(
      (selector) => page.locator(selector),
    );
    const ignored = await ignoredRectangles(page, [
      ...request.definition.visual.ignoreSelectors,
      ...step.ignoreSelectors,
    ]);
    await page.screenshot({
      path: currentPath,
      fullPage: step.fullPage,
      animations: "disabled",
      caret: "hide",
      mask: masks,
    });
    const currentBytes = await enforceVerificationArtifactSize(currentPath);
    capPush(
      state.artifacts,
      {
        kind: "visual-current",
        path: currentPath,
        bytes: currentBytes,
        variant: variant.name,
        stepId: step.id,
      },
      MAX_ARTIFACTS,
    );
    if (!step.compare) {
      return {
        variant: variant.name,
        stepId: step.id,
        name: step.name,
        status: "passed",
        width: variant.width,
        height: variant.height,
        diffPixels: 0,
        diffPixelRatio: 0,
        perceptualDifference: 0,
        thresholds: {
          pixelThreshold: request.definition.visual.pixelThreshold,
          maxDiffPixelRatio: request.definition.visual.maxDiffPixelRatio,
          perceptualThreshold: request.definition.visual.perceptualThreshold,
        },
        baselinePath: "",
        currentPath,
        baselineHistory: [],
      };
    }
    const baselineDirectory = await ensureRoot(
      join(baselineRoot, request.definition.id, variant.name),
    );
    const baselinePath = join(baselineDirectory, `${step.name}.png`);
    const historyDirectory = await ensureRoot(join(baselineDirectory, "history", step.name));
    const baselineExists = await fileExists(baselinePath);
    // Core owns baseline approval. On a first capture it deliberately invokes
    // the service with updateBaseline="never", then persists the current image
    // as reviewable evidence. Returning a `created` comparison (without
    // writing an unapproved baseline) is what makes approve -> materialize ->
    // compare possible across the Rust/Node boundary.
    if (!baselineExists && request.definition.visual.updateBaseline === "never") {
      const result: VisualComparison = {
        variant: variant.name,
        stepId: step.id,
        name: step.name,
        status: "created",
        width: variant.width,
        height: variant.height,
        diffPixels: 0,
        diffPixelRatio: 0,
        perceptualDifference: 0,
        thresholds: {
          pixelThreshold: request.definition.visual.pixelThreshold,
          maxDiffPixelRatio: request.definition.visual.maxDiffPixelRatio,
          perceptualThreshold: request.definition.visual.perceptualThreshold,
        },
        baselinePath,
        currentPath,
        baselineHistory: [],
      };
      capPush(state.visuals, result, MAX_VISUAL_COMPARISONS);
      return result;
    }
    if (!baselineExists || request.definition.visual.updateBaseline === "always") {
      if (baselineExists) {
        const hash = createHash("sha256")
          .update(await readFile(baselinePath))
          .digest("hex")
          .slice(0, 12);
        await copyFile(baselinePath, join(historyDirectory, `${Date.now()}-${hash}.png`));
      }
      await copyFile(currentPath, baselinePath);
      const baselineBytes = await enforceVerificationArtifactSize(baselinePath);
      capPush(
        state.artifacts,
        {
          kind: "visual-baseline",
          path: baselinePath,
          bytes: baselineBytes,
          variant: variant.name,
          stepId: step.id,
        },
        MAX_ARTIFACTS,
      );
      const result: VisualComparison = {
        variant: variant.name,
        stepId: step.id,
        name: step.name,
        status: baselineExists ? "updated" : "created",
        width: variant.width,
        height: variant.height,
        diffPixels: 0,
        diffPixelRatio: 0,
        perceptualDifference: 0,
        thresholds: {
          pixelThreshold: request.definition.visual.pixelThreshold,
          maxDiffPixelRatio: request.definition.visual.maxDiffPixelRatio,
          perceptualThreshold: request.definition.visual.perceptualThreshold,
        },
        baselinePath,
        currentPath,
        baselineHistory: await historyPaths(historyDirectory),
      };
      capPush(state.visuals, result, MAX_VISUAL_COMPARISONS);
      return result;
    }
    const current = PNG.sync.read(await readFile(currentPath));
    const baseline = PNG.sync.read(await readFile(baselinePath));
    const dimensionsMatch = current.width === baseline.width && current.height === baseline.height;
    const diff = new PNG({ width: current.width, height: current.height });
    let diffPixels = current.width * current.height;
    let perceptual = 1;
    if (dimensionsMatch) {
      rectanglePixels(baseline, ignored, current);
      diffPixels = pixelmatch(
        baseline.data,
        current.data,
        diff.data,
        current.width,
        current.height,
        {
          threshold: request.definition.visual.pixelThreshold,
          includeAA: false,
        },
      );
      perceptual = perceptualDifference(baseline, current);
    } else {
      diff.data.fill(255);
    }
    const diffPixelRatio = diffPixels / Math.max(1, current.width * current.height);
    const diffPath = join(variantDirectory, `${step.name}-diff.png`);
    await writeFile(diffPath, PNG.sync.write(diff));
    const diffBytes = await enforceVerificationArtifactSize(diffPath);
    capPush(
      state.artifacts,
      {
        kind: "visual-baseline",
        path: baselinePath,
        bytes: await enforceVerificationArtifactSize(baselinePath),
        variant: variant.name,
        stepId: step.id,
      },
      MAX_ARTIFACTS,
    );
    capPush(
      state.artifacts,
      {
        kind: "visual-diff",
        path: diffPath,
        bytes: diffBytes,
        variant: variant.name,
        stepId: step.id,
      },
      MAX_ARTIFACTS,
    );
    const passed =
      dimensionsMatch &&
      diffPixelRatio <= request.definition.visual.maxDiffPixelRatio &&
      perceptual <= request.definition.visual.perceptualThreshold;
    const result: VisualComparison = {
      variant: variant.name,
      stepId: step.id,
      name: step.name,
      status: passed ? "passed" : "failed",
      width: current.width,
      height: current.height,
      diffPixels,
      diffPixelRatio,
      perceptualDifference: perceptual,
      thresholds: {
        pixelThreshold: request.definition.visual.pixelThreshold,
        maxDiffPixelRatio: request.definition.visual.maxDiffPixelRatio,
        perceptualThreshold: request.definition.visual.perceptualThreshold,
      },
      baselinePath,
      currentPath,
      diffPath,
      baselineHistory: await historyPaths(historyDirectory),
    };
    capPush(state.visuals, result, MAX_VISUAL_COMPARISONS);
    return result;
  }
}
