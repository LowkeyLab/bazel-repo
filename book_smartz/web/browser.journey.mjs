/* global process, URL, getComputedStyle, document, console, window */
import { createInterface } from "node:readline";
import assert from "node:assert/strict";
import { chromium } from "playwright";
import { readFileSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { createServer as httpServer, request } from "node:http";
import { createServer as httpsServer } from "node:https";
import { connect } from "node:net";
import { setTimeout as delay } from "node:timers/promises";

// Real TLS transport preserves native redirects, secure cookies, and form Origin.
// The CONNECT proxy routes only the fixture hostname to this package's loopback app.
const controls = createInterface({ input: process.stdin });
const acknowledgements = controls[Symbol.asyncIterator]();
async function fixture(command) {
  console.log(`fixture:${command}`);
  assert.equal((await acknowledgements.next()).value, "ack");
}
const sockets = new Set();
const upstreamOrigin = new URL(process.argv[2]);
let heldAddResponse;
let addRequests = 0;
const transport = httpsServer(
  { key: readFileSync(process.argv[3]), cert: readFileSync(process.argv[4]) },
  (incoming, outgoing) => {
    if (incoming.method === "POST")
      assert.equal(
        incoming.headers.origin,
        "https://reader.example",
        "Native forms must retain the canonical Origin",
      );
    const held =
      incoming.method === "POST" && incoming.url === "/add"
        ? heldAddResponse
        : null;
    if (incoming.method === "POST" && incoming.url === "/add") addRequests++;
    const upstream = request(
      {
        protocol: upstreamOrigin.protocol,
        hostname: upstreamOrigin.hostname,
        port: upstreamOrigin.port,
        path: incoming.url,
        method: incoming.method,
        headers: incoming.headers,
      },
      async (response) => {
        if (held) {
          held.arrived(response.statusCode);
          await held.release;
        }
        outgoing.writeHead(response.statusCode, response.headers);
        response.pipe(outgoing);
      },
    );
    upstream.on("error", (error) => {
      if (held) held.failed(error);
      outgoing.writeHead(502);
      outgoing.end();
    });
    incoming.pipe(upstream);
  },
);
await new Promise((resolve) => transport.listen(0, "127.0.0.1", resolve));
const proxy = httpServer();
proxy.on("connect", (request, client, head) => {
  if (request.url !== "reader.example:443") {
    client.destroy();
    return;
  }
  const upstream = connect(transport.address().port, "127.0.0.1", () => {
    client.write("HTTP/1.1 200 Connection Established\r\n\r\n");
    if (head.length) upstream.write(head);
    client.pipe(upstream);
    upstream.pipe(client);
  });
  upstream.on("error", () => client.destroy());
  client.on("error", () => upstream.destroy());
  client.on("close", () => upstream.destroy());
});
for (const server of [transport, proxy])
  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
  });
await new Promise((resolve) => proxy.listen(0, "127.0.0.1", resolve));

const executablePath =
  process.env.CHROMIUM_EXECUTABLE ||
  process.env.CHROME_BIN ||
  chromium.executablePath();

const browser = await chromium.launch({
  executablePath,
  args: ["--no-sandbox"],
  proxy: {
    server: `http://127.0.0.1:${proxy.address().port}`,
    bypass: "127.0.0.1,localhost",
  },
});

try {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    ignoreHTTPSErrors: true,
  });
  await context.route("https://covers.openlibrary.org/**", (route) =>
    route.abort(),
  );
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  page.setDefaultNavigationTimeout(10000);
  await page.goto("https://reader.example/");
  const evidence = process.env.TEST_UNDECLARED_OUTPUTS_DIR;
  const scenes = [];
  async function recordScene(name, target = page) {
    const original = target.viewportSize();
    for (const width of [320, 390, 1440]) {
      await target.setViewportSize({ width, height: 900 });
      await target.evaluate(() => document.fonts.ready);
      assert.equal(
        await target.evaluate(
          () => document.documentElement.scrollWidth <= window.innerWidth,
        ),
        true,
        `${name} overflows at ${width}px`,
      );
      const record = {
        name,
        engine: "Chromium",
        version: browser.version(),
        width,
        height: 900,
      };
      scenes.push(record);
      if (evidence) {
        mkdirSync(evidence, { recursive: true });
        await target.screenshot({
          path: join(evidence, `${name}-${width}.png`),
          fullPage: true,
        });
        writeFileSync(
          join(evidence, "browser-evidence.json"),
          JSON.stringify(scenes, null, 2),
        );
      }
    }
    await target.setViewportSize(original);
  }
  await recordScene("sign-in");
  const typography = await page.evaluate(() => ({
    body: getComputedStyle(document.body).fontFamily,
    heading: getComputedStyle(document.querySelector("h1")).fontFamily,
    loaded: [...document.fonts]
      .filter((font) => font.status === "loaded")
      .map((font) => font.family.replaceAll('"', "")),
  }));
  assert.match(typography.body, /Source Sans 3/);
  assert.match(typography.heading, /Literata/);
  assert.equal(typography.loaded.includes("Literata"), true);
  assert.equal(typography.loaded.includes("Source Sans 3"), true);
  const signIn = page.getByRole("link", { name: "Sign in with Google" });
  await signIn.focus();
  assert.notEqual(
    await signIn.evaluate((link) => getComputedStyle(link).outlineStyle),
    "none",
  );
  const native = await browser.newContext({
    javaScriptEnabled: false,
    ignoreHTTPSErrors: true,
    viewport: { width: 320, height: 900 },
  });
  const nativePage = await native.newPage();
  let nativeActions = 0;
  nativePage.on("request", (request) => {
    if (
      request.method() === "POST" &&
      [
        "/search",
        "/select",
        "/add",
        "/answer",
        "/pause",
        "/resume",
        "/recover",
      ].includes(new URL(request.url()).pathname)
    )
      nativeActions++;
  });
  await nativePage.goto("https://reader.example/");
  await nativePage.getByRole("link", { name: "Sign in with Google" }).click();
  await nativePage.waitForURL("https://reader.example/ranking");
  assert.equal(
    nativeActions,
    0,
    "Authentication must not submit an earlier action",
  );
  await native.close();
  await page.getByRole("link", { name: "Sign in with Google" }).click();
  await page.waitForURL("https://reader.example/ranking");
  await recordScene("empty-library");
  const savedHeading = page.getByRole("heading", {
    name: "Your saved ranking",
    exact: true,
  });
  const taskHeading = page.getByRole("heading", {
    name: "Find your next book",
    exact: true,
  });
  assert.equal(
    (await savedHeading.boundingBox()).x < (await taskHeading.boundingBox()).x,
    true,
    "Desktop saved positions must precede the active task visually",
  );
  await page
    .getByRole("textbox", { name: "Title or author" })
    .fill("First book");
  await page.getByRole("button", { name: "Search books", exact: true }).click();
  await page
    .getByAltText("Cover unavailable")
    .waitFor({ state: "visible", timeout: 5000 });
  assert.equal(await page.getByAltText("Cover unavailable").count(), 1);
  await page.getByRole("button", { name: "Select this work" }).click();
  await page.waitForLoadState("domcontentloaded");
  await page
    .getByText("Fixture Author", { exact: true })
    .waitFor({ state: "visible" });
  await page.getByAltText("Cover unavailable").waitFor({ state: "visible" });
  assert.equal(
    await page
      .getByRole("heading", { name: "Selected work", exact: true })
      .evaluate((heading) => document.activeElement === heading),
    true,
  );
  await recordScene("selected-work");
  assert.equal(
    await page.getByRole("status").count(),
    1,
    "Add needs an existing live region before submission",
  );
  assert.equal(await page.getByRole("status").textContent(), "");
  assert.equal(
    await page.getByRole("status").getAttribute("aria-live"),
    "polite",
  );

  let releaseAdd;
  const heldAdd = new Promise((resolve) => {
    releaseAdd = resolve;
  });
  let arrivedAdd, rejectedAdd;
  const serverAdd = new Promise((resolve, reject) => {
    arrivedAdd = resolve;
    rejectedAdd = reject;
  });
  heldAddResponse = {
    arrived: arrivedAdd,
    failed: rejectedAdd,
    release: heldAdd,
  };
  let captureAdd;
  const submittedAdd = new Promise((resolve) => {
    captureAdd = resolve;
  });
  await page.exposeFunction("captureAddSubmission", (state) =>
    captureAdd(state),
  );
  await page.evaluate(() => {
    let captured = false;
    let duplicatePrevented = false;
    document.addEventListener("submit", (event) => {
      const form = event.target;
      if (form.getAttribute("action") !== "/add") return;
      if (captured) {
        duplicatePrevented = event.defaultPrevented;
        return;
      }
      captured = true;
      form.dispatchEvent(
        new window.SubmitEvent("submit", { bubbles: true, cancelable: true }),
      );
      const status = form.querySelector('[role="status"]');
      window.captureAddSubmission({
        disabled: [...form.querySelectorAll("button")].every(
          (button) => button.disabled,
        ),
        busy: form.getAttribute("aria-busy"),
        status: status.textContent,
        focusOffStatus: document.activeElement !== status,
        saved: document.querySelectorAll("ol li").length,
        duplicatePrevented,
      });
    });
  });

  const addClick = page
    .getByRole("button", { name: "Add to your ranking" })
    .click({ noWaitAfter: true });
  try {
    const [state, serverStatus] = await Promise.race([
      Promise.all([submittedAdd, serverAdd]),
      delay(10000).then(() => {
        throw new Error("Held native Add did not reach the controlled server");
      }),
    ]);
    assert.deepEqual(state, {
      disabled: true,
      busy: "true",
      status: "Saving…",
      focusOffStatus: true,
      saved: 0,
      duplicatePrevented: true,
    });
    assert.equal(serverStatus, 303);
    assert.equal(addRequests, 1);
    console.log(
      "Held native Add: live feedback, no confirmation, duplicate prevented.",
    );
  } finally {
    heldAddResponse = null;
    releaseAdd();
  }
  await addClick;
  await page.waitForURL("https://reader.example/ranking");
  assert.equal(await page.locator("ol li").count(), 1);
  // Advance past the deployment's one-search-per-second admission deadline.
  await page.waitForTimeout(1100);
  await page
    .getByRole("textbox", { name: "Title or author" })
    .fill("Later book");
  await page.getByRole("button", { name: "Search books", exact: true }).click();
  await page.getByRole("button", { name: "Select this work" }).click();
  await page.getByRole("button", { name: "Add to your ranking" }).click();
  await page.waitForURL("https://reader.example/ranking");
  assert.equal(await page.locator("ol li").count(), 1);
  assert.equal(await page.locator("ol").getByText("Later book").count(), 0);
  assert.equal(await page.getByRole("button", { name: /Skip/ }).count(), 0);
  const columns = () =>
    page
      .locator("main")
      .evaluate(
        (main) => getComputedStyle(main).gridTemplateColumns.split(" ").length,
      );
  assert.equal(await columns(), 2);
  const stale = await context.newPage();
  stale.setDefaultTimeout(10000);
  stale.setDefaultNavigationTimeout(10000);
  await stale.goto("https://reader.example/ranking");
  await fixture("reject");
  const pause = page.getByRole("button", {
    name: "Pause for now",
    exact: true,
  });
  await pause.focus();
  await pause.press("Enter");
  assert.equal(
    await page.getByRole("status").filter({ hasText: "Checking" }).isVisible(),
    true,
  );
  await page.getByRole("link", { name: "Return to your ranking" }).click();
  assert.equal(
    await page
      .getByRole("button", { name: "Pause for now", exact: true })
      .isDisabled(),
    true,
  );
  assert.equal(
    await page
      .getByRole("button", { name: "Prefer Later book", exact: true })
      .isDisabled(),
    true,
  );
  await fixture("allow");
  await page.getByRole("button", { name: "Check original save" }).click();
  await page.waitForURL("https://reader.example/ranking");
  assert.equal(
    await page.getByText("Your unfinished comparison is paused.").isVisible(),
    true,
  );
  await page.reload();
  await fixture("expire");
  const expired = page.waitForResponse(
    (response) =>
      response.url().endsWith("/resume") &&
      response.request().method() === "POST",
  );
  await page.getByRole("button", { name: "Resume", exact: true }).click();
  assert.equal((await expired).status(), 401);
  await page.goto("https://reader.example/");
  await page.getByRole("link", { name: "Sign in with Google" }).click();
  await page.waitForURL("https://reader.example/ranking");
  assert.equal(
    await page.getByText("Your unfinished comparison is paused.").isVisible(),
    true,
  );
  await page.getByRole("button", { name: "Resume", exact: true }).click();
  await page.waitForURL("https://reader.example/ranking");
  assert.equal(await page.locator("ol li").count(), 1);
  await stale.reload();
  await page.setViewportSize({ width: 390, height: 844 });
  assert.equal(await columns(), 1);
  const prefer = page.getByRole("button", {
    name: "Prefer Later book",
    exact: true,
  });
  await prefer.focus();
  assert.equal(
    await prefer.evaluate(
      (button) =>
        document.activeElement === button &&
        getComputedStyle(button).outlineStyle !== "none",
    ),
    true,
  );
  let capture;
  const submitted = new Promise((resolve, reject) => {
    capture = resolve;
    page.once("pageerror", reject);
  });
  await page.exposeFunction("captureSubmission", (state) => capture(state));
  await page.evaluate(() => {
    document.addEventListener("submit", (event) => {
      const form = event.target;
      if (form.getAttribute("action") === "/answer")
        window.captureSubmission({
          disabled: [...form.querySelectorAll("button")].every(
            (button) => button.disabled,
          ),
          submitting: !form.querySelector('[role="status"]').hidden,
        });
    });
  });
  const navigation = page.waitForNavigation();
  await prefer.press("Enter", { noWaitAfter: true });
  const state = await submitted;
  assert.deepEqual(state, { disabled: true, submitting: true });
  await navigation;
  await page.waitForFunction(
    () => document.querySelectorAll("ol li").length === 2,
  );
  const titles = await page.locator("ol li a").allTextContents();
  assert.deepEqual(titles, ["Later book", "First book"]);
  await stale
    .getByRole("button", { name: "Prefer First book", exact: true })
    .click();
  await stale
    .getByRole("status")
    .filter({ hasText: "comparison changed" })
    .waitFor({ state: "visible", timeout: 5000 });
  await page.reload();
  assert.deepEqual(await page.locator("ol li a").allTextContents(), titles);
  await page.waitForTimeout(1100);
  await page
    .getByRole("textbox", { name: "Title or author" })
    .fill("Unavailable");
  await page.getByRole("button", { name: "Search books", exact: true }).click();
  await page
    .getByRole("button", { name: /Retry/ })
    .waitFor({ state: "visible", timeout: 5000 });
  await recordScene("provider-error");
  let searchRequests = 0;
  page.on("request", (request) => {
    if (
      request.method() === "POST" &&
      new URL(request.url()).pathname === "/search"
    )
      searchRequests++;
  });
  async function deliberateSearch(query) {
    const before = searchRequests;
    await page.getByRole("textbox", { name: "Title or author" }).fill(query);
    await page.waitForTimeout(1100);
    assert.equal(searchRequests, before, "Typing must not start a search");
    await page.getByRole("button", { name: /Search books|Retry/ }).click();
  }
  await deliberateSearch("Pages");
  await page.getByRole("link", { name: "Page one", exact: true }).waitFor();
  assert.equal(
    await page.getByText("Author unavailable", { exact: true }).count(),
    1,
  );
  await page.waitForTimeout(1100);
  await page.getByRole("button", { name: "Next page", exact: true }).click();
  await page.getByRole("link", { name: "Page two", exact: true }).waitFor();
  assert.equal(
    await page
      .getByRole("button", { name: "Previous page", exact: true })
      .isVisible(),
    true,
  );
  assert.equal(
    await page.getByRole("button", { name: "Next page", exact: true }).count(),
    0,
  );
  await recordScene("pagination");
  await deliberateSearch("Long title");
  await page.getByRole("link", { name: /^A long title/ }).waitFor();
  await recordScene("long-title-missing-metadata");
  await page
    .getByRole("button", { name: "Select this work", exact: true })
    .click();
  await page
    .getByRole("heading", { name: "Selected work", exact: true })
    .waitFor();
  await recordScene("selected-long-title");
  await page.getByRole("link", { name: "Return to your ranking" }).click();
  await deliberateSearch("No matches");
  await page.getByText("No matches found.", { exact: true }).waitFor();
  await recordScene("empty-results");
  assert.equal(
    await page
      .getByRole("button", { name: "Select this work", exact: true })
      .count(),
    0,
  );
  console.log("Browser version:", browser.version());
  console.log(
    "Browser journey passed: sign-in, search, first/later Add, keyboard preference, stale tab, mobile layout, reload.",
  );
} finally {
  controls.close();
  await browser.close();
  for (const socket of sockets) socket.destroy();
  proxy.close();
  transport.close();
}
