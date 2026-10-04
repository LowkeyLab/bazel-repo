import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { resolve, extname, relative, sep, join } from "node:path";
import {
  chromium,
  type BrowserContext,
  type BrowserContextOptions,
  type Page,
} from "playwright";
import type { SectionAnimationSettled } from "../../src/utils/animation-observation.js";

interface PageOptions {
  route: string;
  viewport?: BrowserContextOptions["viewport"];
  reducedMotion?: BrowserContextOptions["reducedMotion"];
  javaScriptEnabled?: boolean;
  hasTouch?: boolean;
  fontMode?: "blocked" | "held" | "delayed";
  holdAnimationModules?: boolean;
  blockAnimationModule?: boolean;
  clock?: boolean;
  trackPending?: boolean;
  initialPaintSelector?: string;
  recordAnimationEvents?: boolean;
  throwAnimationListener?: boolean;
}

interface PageControls {
  releaseFonts: () => void;
  releaseModules: () => void;
}

export interface HeroObservation {
  root: Element | null;
  roots: number;
  writingStarts: number;
  reveals: number;
  regressions: number;
  writing: boolean;
  revealed: boolean;
  stopped: boolean;
}

declare global {
  interface Window {
    __pendingStartedAt?: number;
    __initialPaintSamples: { opacity: number; visibility: string }[];
    __animationRecords: {
      level: "debug" | "warn";
      event: SectionAnimationSettled;
    }[];
    __heroObservation?: HeroObservation;
    __motionPreferenceChanged?: Promise<void>;
  }
}

function runfile(value: string | undefined) {
  assert.ok(value, "missing declared Bazel runfile");
  return resolve(
    process.env.RUNFILES_DIR ?? process.cwd(),
    process.env.TEST_WORKSPACE ?? "",
    value,
  );
}

const fixtures = runfile(process.env.SITE_DIR ?? process.env.FIXTURE_DIR);
const contentTypes: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".svg": "image/svg+xml",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".png": "image/png",
  ".woff2": "font/woff2",
};

function server() {
  return createServer(async (request, response) => {
    try {
      assert.ok(request.url, "request URL is required");
      const pathname = new URL(request.url, "http://localhost").pathname;
      const target = resolve(
        fixtures,
        `.${pathname}`,
        pathname.endsWith("/") ? "index.html" : "",
      );
      const fromRoot = relative(fixtures, target);
      if (fromRoot.startsWith(`..${sep}`) || fromRoot === "..") {
        response.writeHead(403).end();
        return;
      }
      const body = await readFile(target);
      response
        .writeHead(200, {
          "content-type":
            contentTypes[extname(target)] ?? "application/octet-stream",
        })
        .end(body);
    } catch {
      response.writeHead(404).end();
    }
  });
}

export async function withPage(
  options: PageOptions,
  run: (page: Page, controls: PageControls) => Promise<void>,
) {
  const http = server();
  const browser = await chromium.launch({
    headless: true,
    ...(process.env.CHROMIUM_EXECUTABLE
      ? { executablePath: process.env.CHROMIUM_EXECUTABLE }
      : {}),
    args: ["--no-sandbox"],
  });
  let context: BrowserContext | undefined;
  let page: Page | undefined;
  let releaseFonts!: () => void;
  let releaseModules!: () => void;
  const fontsHeld = new Promise<void>((resolve) => (releaseFonts = resolve));
  const modulesHeld = new Promise<void>(
    (resolve) => (releaseModules = resolve),
  );
  try {
    await new Promise<void>((done, fail) =>
      http.once("error", fail).listen(0, "127.0.0.1", done),
    );
    const address = http.address();
    assert.ok(
      address && typeof address === "object",
      "server must listen on TCP",
    );
    const port = address.port;
    context = await browser.newContext({
      viewport: options.viewport ?? { width: 1280, height: 900 },
      reducedMotion: options.reducedMotion ?? "no-preference",
      javaScriptEnabled: options.javaScriptEnabled ?? true,
      serviceWorkers: "block",
      hasTouch: options.hasTouch ?? false,
    });
    // Fault cases delay/block the actual embedded production bytes by exposing
    // them at a test-server URL. Normal tests leave the stylesheet untouched.
    const embeddedFonts = new Map<string, Buffer>();
    await context.route("**/__font_fault/*", async (route) => {
      if (options.fontMode === "blocked") return route.abort();
      if (options.fontMode === "held") await fontsHeld;
      if (options.fontMode === "delayed")
        await new Promise<void>((resolve) => setTimeout(resolve, 2600));
      const name = new URL(route.request().url()).pathname;
      const body = embeddedFonts.get(name);
      assert.ok(body, "fault injection must serve a real production font");
      await route.fulfill({ status: 200, contentType: "font/woff2", body });
    });
    if (options.fontMode) {
      await context.route("**/*.css", async (route) => {
        const response = await route.fetch();
        const body = (await response.text()).replace(
          /data:font\/woff2;base64,([A-Za-z0-9+/=]+)/g,
          (_, data) => {
            const path = `/__font_fault/${embeddedFonts.size}.woff2`;
            embeddedFonts.set(path, Buffer.from(data, "base64"));
            return path;
          },
        );
        await route.fulfill({ response, body });
      });
    }
    await context.route("**/*", async (route) => {
      const url = new URL(route.request().url());
      if (options.holdAnimationModules && url.pathname.endsWith(".js"))
        await modulesHeld;
      if (options.blockAnimationModule && url.pathname.endsWith(".js"))
        return route.abort();
      if (url.hostname !== "127.0.0.1") return route.abort();
      return route.fallback();
    });
    page = await context.newPage();
    await context.tracing.start({ screenshots: true, snapshots: true });
    if (options.clock) {
      // install() still ticks in real time. Pause before navigation so slow CI
      // requests and screenshots cannot advance the animation between runFor calls.
      await page.clock.install({ time: 0 });
      await page.clock.pauseAt(60_000);
    }
    if (options.trackPending) {
      await page.addInitScript(() => {
        new MutationObserver((changes) => {
          if (window.__pendingStartedAt !== undefined) return;
          if (
            changes.some(
              (change) =>
                change.type === "attributes" &&
                change.target instanceof Element &&
                change.target.matches(
                  "home-hero-animation[data-animation-pending]",
                ),
            )
          ) {
            window.__pendingStartedAt = performance.now();
          }
        }).observe(document, {
          attributes: true,
          subtree: true,
          attributeFilter: ["data-animation-pending"],
        });
      });
    }
    if (options.initialPaintSelector) {
      await page.addInitScript((selector) => {
        window.__initialPaintSamples = [];
        const sample = () => {
          const element = document.querySelector(selector);
          if (element) {
            const style = getComputedStyle(element);
            window.__initialPaintSamples.push({
              opacity: Number(style.opacity),
              visibility: style.visibility,
            });
          }
          if (window.__initialPaintSamples.length < 120)
            requestAnimationFrame(sample);
        };
        requestAnimationFrame(sample);
      }, options.initialPaintSelector);
    }
    if (options.recordAnimationEvents) {
      await page.addInitScript((throwListener) => {
        window.__animationRecords = [];
        for (const level of ["debug", "warn"] as const) {
          const original = console[level].bind(console);
          console[level] = (...args) => {
            const event = args.find(
              (value) => value?.type === "SectionAnimationSettled",
            );
            if (event) window.__animationRecords.push({ level, event });
            if (event && throwListener)
              throw new Error("simulated listener failure");
            original(...args);
          };
        }
      }, options.throwAnimationListener);
    }
    const errors: Error[] = [];
    page.on("pageerror", (error) => errors.push(error));
    page.on("console", (message) => {
      if (
        message.type() === "error" &&
        !options.blockAnimationModule &&
        options.fontMode !== "blocked"
      )
        errors.push(new Error(message.text()));
    });
    await page.goto(`http://127.0.0.1:${port}${options.route}`, {
      waitUntil: options.holdAnimationModules ? "commit" : "domcontentloaded",
    });
    await run(page, { releaseFonts, releaseModules });
    assert.deepEqual(
      errors.map((error) => String(error)),
      [],
      "browser errors",
    );
  } catch (error) {
    if (page && process.env.TEST_UNDECLARED_OUTPUTS_DIR) {
      await page
        .screenshot({
          path: join(
            process.env.TEST_UNDECLARED_OUTPUTS_DIR,
            `failure-${Date.now()}.png`,
          ),
          fullPage: true,
        })
        .catch(() => {});
      await context?.tracing
        .stop({
          path: join(
            process.env.TEST_UNDECLARED_OUTPUTS_DIR,
            `failure-${Date.now()}.zip`,
          ),
        })
        .catch(() => {});
    }
    throw error;
  } finally {
    releaseFonts();
    releaseModules();
    await context?.tracing.stop().catch(() => {});
    await context?.close();
    await browser.close();
    await new Promise<void>((done) => http.close(() => done()));
  }
}

export async function presentation(page: Page, selector: string) {
  return page.locator(selector).evaluate((element) => {
    const style = getComputedStyle(element);
    const box = element.getBoundingClientRect();
    return {
      opacity: Number(style.opacity),
      visibility: style.visibility,
      box: { x: box.x, y: box.y, width: box.width, height: box.height },
    };
  });
}
