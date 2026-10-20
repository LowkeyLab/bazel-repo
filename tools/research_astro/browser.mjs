import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFileSync, writeFileSync, mkdirSync, readdirSync } from "node:fs";
import { resolve, extname } from "node:path";
const [dist, executable, submitting, maintained] = process.argv
  .slice(2)
  .map((value) => resolve(value));
const output = process.env.TEST_UNDECLARED_OUTPUTS_DIR;
mkdirSync(output, { recursive: true });
const save = (name, value) =>
  writeFileSync(
    resolve(output, name),
    typeof value === "string" ? value : JSON.stringify(value, null, 2),
  );
const submittingText = readFileSync(submitting, "utf8");
const appHtml =
  '<!doctype html><form data-ranking-action="yes"><button type="submit" name="action" value="rank">Rank</button><span data-submitting hidden>Working</span></form><script src="/application/submitting.js"></script>';
const server = createServer((request, response) => {
  let body;
  try {
    if (request.url === "/application/") body = appHtml;
    else if (request.url === "/application/submitting.js")
      body = submittingText;
    else if (request.url === "/maintained/")
      body = readFileSync(resolve(maintained, "index.html"));
    else {
      const path = request.url === "/" ? "index.html" : "." + request.url;
      try {
        body = readFileSync(resolve(dist, path));
      } catch (error) {
        if (!maintained) throw error;
        body = readFileSync(resolve(maintained, path));
      }
    }
    response.setHeader(
      "content-type",
      request.url.endsWith(".js")
        ? "text/javascript"
        : request.url.endsWith(".map")
          ? "application/json"
          : "text/html",
    );
    response.end(body);
  } catch {
    response.writeHead(404);
    response.end("missing");
  }
});
await new Promise((done) => server.listen(0, "127.0.0.1", done));
const origin = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({
  executablePath: executable,
  headless: true,
  args: ["--no-sandbox"],
});
save("browser-version.json", {
  version: browser.version(),
  runtime: process.version,
  scope: "top-level page only",
});
async function interval(name, route, act, interrupt = false) {
  const page = await browser.newPage();
  const cdp = await page.context().newCDPSession(page);
  const scripts = [];
  cdp.on("Debugger.scriptParsed", (event) => scripts.push(event));
  await cdp.send("Debugger.enable");
  await cdp.send("Profiler.enable");
  await cdp.send("Profiler.startPreciseCoverage", {
    callCount: true,
    detailed: true,
  });
  await page.goto(origin + route);
  await act(page);
  save(`${name}-functional.json`, { passed: true });
  if (interrupt) {
    await page.close();
    try {
      await cdp.send("Profiler.takePreciseCoverage");
    } catch (error) {
      save(`${name}-interrupted.txt`, String(error));
    }
    return;
  }
  const profile = await cdp.send("Profiler.takePreciseCoverage");
  const selected = [];
  for (const entry of profile.result) {
    if (!(
      entry.url.startsWith(origin) || entry.url === "research-evaluated.js"
    ))
      continue;
    const parsed = scripts.find((item) => item.scriptId === entry.scriptId);
    const source = await cdp.send("Debugger.getScriptSource", {
      scriptId: entry.scriptId,
    });
    const stem = `${name}-script-${selected.length}`;
    save(`${stem}.js`, source.scriptSource);
    selected.push({ ...entry, parsed, textFile: `${stem}.js` });
  }
  save(`${name}-profile.json`, { ...profile, result: selected });
  save(`${name}-completion.json`, {
    completed: true,
    beforeClosure: true,
    scripts: selected.length,
  });
  await cdp.send("Profiler.stopPreciseCoverage");
  await page.close();
}
try {
  await interval("astro", "/", async (page) => {
    if ((await page.locator("h1").textContent()) !== "server-yes")
      throw Error("Prerender assertion failed.");
    await page.locator("#action").click();
    if (
      (await page.evaluate(() => document.documentElement.dataset.result)) !==
      "browser-yes"
    )
      throw Error("Processed script assertion failed.");
    if ((await page.evaluate(() => window.inlineResult)) !== "inline-yes")
      throw Error("Inline script assertion failed.");
    const code = readFileSync(
      "tools/research_astro/fixtures/evaluated.js",
      "utf8",
    );
    await page.evaluate(code + "\n//# sourceURL=research-evaluated.js");
    if (
      (await page.evaluate(() => globalThis.evaluatedResult)) !==
      "evaluated-yes"
    )
      throw Error("Evaluation assertion failed.");
  });
  if (maintained)
    await interval("maintained", "/maintained/", async (page) => {
      if (
        !(await page.locator("#intro-heading").textContent()).includes("Tim.")
      )
        throw Error("Maintained homepage assertion failed.");
      await page.locator(".hero-replay").click();
      await page.waitForFunction(
        () =>
          Number(
            getComputedStyle(document.querySelector(".hero-code")).opacity,
          ) < 0.99,
      );
      save("maintained-route.json", {
        url: page.url(),
        artifact: maintained,
        replayStarted: true,
      });
    });
  await interval("application", "/application/", async (page) => {
    await page.evaluate(() => {
      document
        .querySelector("form")
        .addEventListener("submit", (event) => event.preventDefault());
    });
    await page.locator("button").click();
    if ((await page.locator("form").getAttribute("aria-busy")) !== "true")
      throw Error("Application submission assertion failed.");
    if (!(await page.locator("button").isDisabled()))
      throw Error("Application disable assertion failed.");
  });
  await interval(
    "interrupted",
    "/",
    async (page) => {
      if ((await page.locator("h1").textContent()) !== "server-yes")
        throw Error("Interrupted functional assertion failed.");
    },
    true,
  );
  save("built-index.html", readFileSync(resolve(dist, "index.html"), "utf8"));
  for (const file of readdirSync(resolve(dist, "_astro")))
    if (file.endsWith(".map"))
      save(file, readFileSync(resolve(dist, "_astro", file), "utf8"));
  save("application-submitting.js", submittingText);
  console.log(
    "Browser assertions passed. Complete profiles and interrupted interval retained.",
  );
} finally {
  await browser.close();
  await new Promise((done) => server.close(done));
}
