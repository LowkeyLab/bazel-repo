import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { resolve, extname, relative, sep, join } from "node:path";
import { chromium } from "playwright";

function runfile(value) {
  assert.ok(value, "missing declared Bazel runfile");
  return resolve(
    process.env.RUNFILES_DIR ?? process.cwd(),
    process.env.TEST_WORKSPACE ?? "",
    value,
  );
}

const fixtures = runfile(process.env.FIXTURE_DIR);
const font = runfile(process.env.FONT_FILE);
const contentTypes = {
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

export async function withPage(options, run) {
  const http = server();
  const browser = await chromium.launch({
    headless: true,
    ...(process.env.CHROMIUM_EXECUTABLE
      ? { executablePath: process.env.CHROMIUM_EXECUTABLE }
      : {}),
    args: ["--no-sandbox"],
  });
  let context;
  let page;
  try {
    await new Promise((done, fail) =>
      http.once("error", fail).listen(0, "127.0.0.1", done),
    );
    const port = http.address().port;
    context = await browser.newContext({
      viewport: options.viewport ?? { width: 1280, height: 900 },
      reducedMotion: options.reducedMotion ?? "no-preference",
      javaScriptEnabled: options.javaScriptEnabled ?? true,
      serviceWorkers: "block",
    });
    await context.route("https://fonts.googleapis.com/**", async (route) => {
      if (options.fontMode === "blocked") return route.abort();
      if (options.fontMode === "delayed")
        await new Promise((resolve) => setTimeout(resolve, 2600));
      await route.fulfill({
        status: 200,
        contentType: "text/css",
        body: "@font-face{font-family:Caveat;font-style:normal;font-weight:100 900;src:url(https://fonts.gstatic.com/caveat-test.ttf) format('truetype')}",
      });
    });
    await context.route("https://fonts.gstatic.com/**", async (route) => {
      if (options.fontMode === "blocked") return route.abort();
      await route.fulfill({
        status: 200,
        contentType: "font/ttf",
        body: await readFile(font),
      });
    });
    await context.route("**/*", (route) => {
      const url = new URL(route.request().url());
      if (options.blockAnimationModule && url.pathname.endsWith(".js"))
        return route.abort();
      if (
        !["127.0.0.1", "fonts.googleapis.com", "fonts.gstatic.com"].includes(
          url.hostname,
        )
      )
        return route.abort();
      return route.fallback();
    });
    page = await context.newPage();
    await context.tracing.start({ screenshots: true, snapshots: true });
    if (options.clock) await page.clock.install();
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
    const errors = [];
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
      waitUntil: "domcontentloaded",
    });
    await run(page);
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
    await context?.tracing.stop().catch(() => {});
    await context?.close();
    await browser.close();
    await new Promise((done) => http.close(done));
  }
}

export async function presentation(page, selector) {
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
