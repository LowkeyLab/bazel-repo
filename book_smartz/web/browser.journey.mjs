/* global process, URL, getComputedStyle, document, console, window */
import { createLifecycle, runJourney } from "./browser.lifecycle.mjs";
import { createInterface } from "node:readline";
import assert from "node:assert/strict";
import { chromium } from "playwright";
import { readFileSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { createServer as httpServer, request } from "node:http";
import { createServer as httpsServer } from "node:https";
import { connect } from "node:net";
import { setTimeout as delay } from "node:timers/promises";

const lifecycle = createLifecycle("journey");
const stalledPhase = process.env.BOOK_SMARTZ_BROWSER_STALL_PHASE || "";
assert.ok(
  ["", "assertions", "browser_shutdown", "proxy_shutdown"].includes(
    stalledPhase,
  ),
);
lifecycle.record("runner", "start");

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
let heldResponse;
let addRequests = 0;
const mutationRequests = new Map();
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
      incoming.method === "POST" && incoming.url === heldResponse?.path
        ? heldResponse
        : null;
    if (incoming.method === "POST" && incoming.url === "/add") addRequests++;
    if (incoming.method === "POST")
      mutationRequests.set(
        incoming.url,
        (mutationRequests.get(incoming.url) ?? 0) + 1,
      );
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

let browser;
async function journey() {
  browser = await chromium.launch({
    executablePath,
    args: ["--no-sandbox"],
    proxy: {
      server: `http://127.0.0.1:${proxy.address().port}`,
      bypass: "127.0.0.1,localhost",
    },
  });
  lifecycle.record("runner", "complete");
  lifecycle.record("assertions", "start");
  if (process.env.BOOK_SMARTZ_BROWSER_FAIL_ASSERTION === "1")
    assert.fail("Controlled assertion failure");
  if (stalledPhase === "assertions")
    await new Promise(() => {
      /* Exercise the child supervisor deadline. */
    });
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    ignoreHTTPSErrors: true,
    reducedMotion: "no-preference",
  });
  const motions = [];
  let observeInterruption;
  let finishFirstHighlight;
  const firstHighlight = new Promise((resolve) => {
    finishFirstHighlight = resolve;
  });
  await context.exposeFunction("observeRankingMotion", (record) => {
    motions.push(record);
    if (record.type === "transitionend" && record.title === "First book")
      finishFirstHighlight(record);
    return observeInterruption?.(record);
  });
  await context.addInitScript(() => {
    for (const type of ["transitionrun", "transitionend"]) {
      document.addEventListener(type, (event) => {
        const row = event.target.closest?.("ol li");
        if (!row) return;
        const style = getComputedStyle(row, event.pseudoElement || null);
        window.observeRankingMotion({
          type,
          title: row.querySelector("a")?.textContent,
          property: event.propertyName,
          duration: style.transitionDuration,
          easing: style.transitionTimingFunction,
          elapsed: event.elapsedTime,
          bounds: {
            x: row.getBoundingClientRect().x,
            y: row.getBoundingClientRect().y,
            width: row.getBoundingClientRect().width,
            height: row.getBoundingClientRect().height,
          },
        });
      });
    }
    document.addEventListener("input", (event) => {
      if (event.target.value !== "Available during highlight") return;
      const row = [...document.querySelectorAll("ol li")].find(
        (row) => row.querySelector("a")?.textContent === "Interrupted book",
      );
      window.observeRankingMotion({
        type: "input",
        trusted: event.isTrusted,
        running:
          row
            ?.getAnimations({ subtree: true })
            .some((animation) => animation.playState === "running") ?? false,
      });
    });
  });
  const coverRequests = [];
  await context.route("https://covers.openlibrary.org/**", (route) => {
    coverRequests.push(route.request().url());
    return route.abort();
  });
  const page = await context.newPage();
  let firstSearchAt;
  page.on("request", (request) => {
    if (
      firstSearchAt === undefined &&
      request.method() === "POST" &&
      new URL(request.url()).pathname === "/search"
    )
      firstSearchAt = Date.now();
  });
  page.setDefaultTimeout(10000);
  page.setDefaultNavigationTimeout(10000);
  await page.goto("https://reader.example/");
  const evidence = process.env.TEST_UNDECLARED_OUTPUTS_DIR;
  const scenes = [];
  // Chromium supplies computed styles. Node measures their contrast.
  function rgba(value) {
    const parts = value.match(/[\d.]+/g).map(Number);
    return [parts[0], parts[1], parts[2], parts[3] ?? 1];
  }
  function blend(front, back) {
    return front
      .slice(0, 3)
      .map((part, index) => part * front[3] + back[index] * (1 - front[3]));
  }
  function background(chain) {
    let color = [255, 255, 255];
    for (const layer of chain) {
      color = blend(rgba(layer.background), color);
      if (layer.content !== "none" && layer.content !== "normal") {
        const tint = rgba(layer.tint);
        tint[3] *= Number(layer.opacity);
        color = blend(tint, color);
      }
    }
    return color;
  }
  function luminance(color) {
    return color
      .map((value) => {
        const channel = value / 255;
        return channel <= 0.04045
          ? channel / 12.92
          : ((channel + 0.055) / 1.055) ** 2.4;
      })
      .reduce(
        (sum, value, index) => sum + value * [0.2126, 0.7152, 0.0722][index],
        0,
      );
  }
  function contrast(foreground, back) {
    const first = luminance(foreground),
      second = luminance(back);
    return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
  }
  function measureAppearance(elements) {
    const controls = elements
      .filter((element) => element.control)
      .map((element) => ({
        name: element.controlName,
        visible: element.visible,
        ...element.bounds,
      }));
    const measurements = [];
    for (const element of elements.filter((element) => element.visible)) {
      const back = background(element.chain);
      if (element.name) {
        const foreground = blend(rgba(element.color), back);
        const large =
          parseFloat(element.fontSize) >= 24 ||
          (parseFloat(element.fontSize) >= 18.6667 &&
            Number(element.fontWeight) >= 700);
        measurements.push({
          kind: "text",
          name: element.name,
          foreground,
          background: back,
          fontSize: element.fontSize,
          fontWeight: element.fontWeight,
          disabled: element.disabled,
          ratio: contrast(foreground, back),
          minimum: large ? 3 : 4.5,
        });
      }
      const adjacent = background(element.chain.slice(0, -1));
      if (["BUTTON", "INPUT"].includes(element.tag)) {
        const foreground = blend(rgba(element.border), adjacent);
        measurements.push({
          kind: "control boundary",
          name: element.name,
          foreground,
          background: adjacent,
          disabled: element.disabled,
          ratio: contrast(foreground, adjacent),
          minimum: 3,
        });
      }
      if (
        element.outlineStyle !== "none" &&
        parseFloat(element.outlineWidth) > 0
      ) {
        const foreground = blend(rgba(element.outlineColor), adjacent);
        measurements.push({
          kind: "focus outline",
          name: element.name,
          foreground,
          background: adjacent,
          ratio: contrast(foreground, adjacent),
          minimum: 3,
        });
      }
    }
    return { controls, measurements };
  }
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
      const appearance = await target.evaluate(() =>
        [
          ...document.querySelectorAll(
            'h1,h2,h3,p,a,label,legend,button,input:not([type="hidden"])',
          ),
        ].map((element) => {
          const style = getComputedStyle(element);
          const rect = element.getBoundingClientRect();
          const chain = [];
          for (let node = element; node; node = node.parentElement) {
            const decoration = getComputedStyle(node, "::before");
            chain.unshift({
              background: getComputedStyle(node).backgroundColor,
              tint: decoration.backgroundColor,
              opacity: decoration.opacity,
              content: decoration.content,
            });
          }
          return {
            tag: element.tagName,
            name: (
              element.textContent ||
              element.value ||
              element.placeholder ||
              ""
            ).trim(),
            controlName: (
              element.textContent ||
              element.getAttribute("aria-label") ||
              element.name ||
              ""
            ).trim(),
            control: element.matches(
              'a[href],button,input:not([type="hidden"])',
            ),
            visible: element.checkVisibility({
              checkOpacity: true,
              checkVisibilityCSS: true,
            }),
            disabled: element.matches(":disabled"),
            bounds: {
              x: rect.x,
              right: rect.right,
              width: rect.width,
              height: rect.height,
            },
            color: style.color,
            fontSize: style.fontSize,
            fontWeight: style.fontWeight,
            border: style.borderTopColor,
            outlineStyle: style.outlineStyle,
            outlineWidth: style.outlineWidth,
            outlineColor: style.outlineColor,
            chain,
          };
        }),
      );
      const audit = measureAppearance(appearance);
      for (const control of audit.controls) {
        assert.equal(
          control.visible && control.width > 0 && control.height > 0,
          true,
          `${name}: hidden control ${control.name}`,
        );
        assert.equal(
          control.x >= -1 && control.right <= width + 1,
          true,
          `${name}: clipped control ${control.name}`,
        );
      }
      for (const measurement of audit.measurements)
        assert.equal(
          measurement.ratio >= measurement.minimum,
          true,
          `${name}: ${measurement.kind} contrast ${measurement.ratio} for ${measurement.name}`,
        );
      const record = {
        commit: process.env.BOOK_SMARTZ_ACCEPTANCE_COMMIT || "working-tree",
        name,
        ...audit,
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
  assert.equal(await page.title(), "Book Smartz · Books, ranked, not rated");
  assert.equal(
    await page
      .getByRole("heading", { name: "Books, ranked, not rated.", exact: true })
      .isVisible(),
    true,
  );
  assert.equal(
    await page
      .getByText(
        "Put the books you read in order of preference. Choose between two books at a time to build your private ranking.",
        { exact: true },
      )
      .isVisible(),
    true,
  );
  assert.equal(
    await page
      .getByText("Find your first book.", {
        exact: true,
      })
      .isVisible(),
    true,
  );
  assert.equal(
    await page
      .getByRole("img", { name: "Book Smartz", exact: true })
      .isVisible(),
    true,
  );
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
  const signIn = page.getByRole("link", {
    name: "Sign in with Google",
    exact: true,
  });
  assert.equal(await signIn.getAttribute("href"), "/auth/google");
  await signIn.focus();
  assert.notEqual(
    await signIn.evaluate((link) => getComputedStyle(link).outlineStyle),
    "none",
  );
  const disabledJavaScript = await browser.newContext({
    javaScriptEnabled: false,
    ignoreHTTPSErrors: true,
    viewport: { width: 320, height: 900 },
  });
  const noticePage = await disabledJavaScript.newPage();
  await noticePage.goto("https://reader.example/");
  await noticePage
    .getByText("Enable JavaScript to use Book Smartz", { exact: true })
    .waitFor();
  await recordScene("javascript-required", noticePage);
  await disabledJavaScript.close();
  assert.equal(
    await page
      .getByText("Enable JavaScript to use Book Smartz", { exact: true })
      .isVisible(),
    false,
  );
  const authentication = await browser.newContext({
    javaScriptEnabled: true,
    ignoreHTTPSErrors: true,
    viewport: { width: 320, height: 900 },
  });
  const authenticationPage = await authentication.newPage();
  let authenticationActions = 0;
  authenticationPage.on("request", (request) => {
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
      authenticationActions++;
  });
  await authenticationPage.goto("https://reader.example/");
  await authenticationPage
    .getByRole("link", { name: "Sign in with Google" })
    .click();
  await authenticationPage.waitForURL("https://reader.example/ranking");
  assert.equal(
    authenticationActions,
    0,
    "Authentication must not submit an earlier action",
  );
  await authenticationPage
    .getByRole("link", { name: "Your profile", exact: true })
    .click();
  assert.equal(await authenticationPage.title(), "Your profile · Book Smartz");
  assert.equal(
    await authenticationPage
      .getByRole("link", { name: "0 ranked books", exact: true })
      .isVisible(),
    true,
  );
  await authentication.close();
  await page.getByRole("link", { name: "Sign in with Google" }).click();
  await page.waitForURL("https://reader.example/ranking");
  await recordScene("empty-library");
  const profileNavigation = page.getByRole("link", {
    name: "Your profile",
    exact: true,
  });
  await profileNavigation.focus();
  assert.notEqual(
    await profileNavigation.evaluate(
      (link) => getComputedStyle(link).outlineStyle,
    ),
    "none",
  );
  await page.keyboard.press("Enter");
  await page.waitForURL("https://reader.example/profile");
  assert.equal(await page.title(), "Your profile · Book Smartz");
  assert.equal(
    await page
      .getByRole("link", { name: "Your profile", exact: true })
      .getAttribute("aria-current"),
    "page",
  );
  await recordScene("empty-profile");
  await fixture("profile-text");
  await page.reload();
  assert.equal(
    await page.locator(".profile-name").textContent(),
    "N".repeat(100),
  );
  assert.equal(
    await page.locator(".profile-biography").textContent(),
    "First paragraph.\n\n<b>Plain text</b> " + "B".repeat(260),
  );
  assert.equal(await page.locator(".profile-biography b").count(), 0);
  assert.equal(
    await page
      .locator(".profile-biography")
      .evaluate((element) => getComputedStyle(element).whiteSpace),
    "pre-wrap",
  );
  await page.getByRole("link", { name: "0 ranked books", exact: true }).focus();
  await recordScene("saved-profile-long-text");
  await page.keyboard.press("Enter");
  await page.waitForURL("https://reader.example/ranking#saved-ranking");
  await page.goto("https://reader.example/ranking");
  assert.equal(await page.title(), "Your books · Book Smartz");
  const rankingNavigation = page.getByRole("navigation", {
    name: "Books",
    exact: true,
  });
  assert.equal(
    await rankingNavigation
      .getByRole("link", { name: "Find a book", exact: true })
      .getAttribute("href"),
    "/ranking",
  );
  assert.equal(
    await rankingNavigation
      .getByRole("link", { name: "Your books", exact: true })
      .getAttribute("href"),
    "/ranking#saved-ranking",
  );
  assert.equal(
    await page
      .getByRole("region", { name: "Books workspace", exact: true })
      .isVisible(),
    true,
  );
  assert.equal(
    await page
      .getByRole("button", { name: "Sign out of this browser", exact: true })
      .isVisible(),
    true,
  );
  assert.equal(
    await page.getByText("You have no books yet.", { exact: true }).isVisible(),
    true,
  );
  assert.equal(
    await page.getByText("Find a book you read.", { exact: true }).isVisible(),
    true,
  );
  const savedHeading = page.getByRole("heading", {
    name: "Your books",
    exact: true,
  });
  const taskHeading = page.getByRole("heading", {
    name: "Find a book",
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
  assert.equal(await page.title(), "Find a book · Book Smartz");
  assert.equal(
    await page
      .getByRole("heading", { name: "Find a book", exact: true })
      .isVisible(),
    true,
  );
  assert.equal(
    await page
      .getByRole("list", { name: "Search results", exact: true })
      .isVisible(),
    true,
  );
  await page
    .getByRole("button", { name: "Select this book", exact: true })
    .click();
  await page.waitForLoadState("domcontentloaded");
  await page
    .getByText("Fixture Author", { exact: true })
    .waitFor({ state: "visible" });
  await page.getByAltText("Cover unavailable").waitFor({ state: "visible" });
  assert.equal(
    await page
      .getByRole("heading", { name: "Selected book", exact: true })
      .evaluate((heading) => document.activeElement === heading),
    true,
  );
  assert.equal(await page.title(), "Selected book · Book Smartz");
  assert.equal(
    await page
      .getByText(
        "You can add this book for 15 minutes after your search. After that, search for the book again.",
        { exact: true },
      )
      .isVisible(),
    false,
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
  heldResponse = {
    path: "/add",
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
    .getByRole("button", { name: "Add to your books" })
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
    heldResponse = null;
    releaseAdd();
  }
  await addClick;
  await page.waitForURL("https://reader.example/ranking");
  assert.equal(await page.locator("ol li").count(), 1);
  assert.equal(
    await page.getByText("You have no books yet.", { exact: true }).count(),
    0,
  );
  await page
    .getByRole("status")
    .filter({ hasText: "We saved this book at position 1." })
    .waitFor();
  const firstMotion = await Promise.race([
    firstHighlight,
    delay(3000).then(() => {
      throw new Error(
        "Pointer completion must highlight the newly ranked book",
      );
    }),
  ]);
  assert.deepEqual(
    {
      property: firstMotion.property,
      duration: firstMotion.duration,
      easing: firstMotion.easing,
    },
    {
      property: "opacity",
      duration: "0.16s",
      easing: "cubic-bezier(0.23, 1, 0.32, 1)",
    },
  );
  assert.equal(Math.abs(firstMotion.elapsed - 0.16) < 0.01, true);
  assert.deepEqual(
    motions
      .filter(
        (motion) =>
          motion.type === "transitionrun" || motion.type === "transitionend",
      )
      .map((motion) => motion.property),
    ["opacity", "opacity"],
  );
  assert.deepEqual(
    motions[0].bounds,
    motions[1].bounds,
    "Completion must not move the saved row",
  );
  const completedMotions = motions.length;

  await page.reload();
  assert.equal(
    await page
      .getByText("We saved this book at position 1.", { exact: true })
      .count(),
    0,
    "Reload must not replay completion feedback",
  );
  // Advance past the deployment's one-search-per-second admission deadline.
  await page.waitForTimeout(1100);
  await page
    .getByRole("textbox", { name: "Title or author" })
    .fill("Later book");
  await page.getByRole("button", { name: "Search books", exact: true }).click();
  await page.getByRole("button", { name: "Select this book" }).click();
  await page.getByRole("button", { name: "Add to your books" }).click();
  await page.waitForURL("https://reader.example/ranking");
  assert.equal(await page.locator("ol li").count(), 1);
  assert.equal(
    await page.getByText("Book added.", { exact: true }).isVisible(),
    true,
  );
  assert.equal(
    await page.getByText(/We saved this book at position/).count(),
    0,
  );
  const pendingPlacement = page.getByRole("complementary", {
    name: "Book comparisons",
    exact: true,
  });
  for (const label of [
    "Compare books",
    "New book",
    "Your book",
    "Choose the book you prefer",
  ]) {
    assert.equal(
      await pendingPlacement.getByText(label, { exact: true }).isVisible(),
      true,
    );
  }
  await pendingPlacement
    .getByText("Fixture Author", { exact: true })
    .first()
    .waitFor();
  assert.equal(
    await pendingPlacement
      .getByRole("heading", { name: "Which book do you prefer?", exact: true })
      .evaluate((heading) => document.activeElement === heading),
    true,
  );
  for (const endpoint of ["/answer", "/pause"]) {
    const status = pendingPlacement
      .locator(`form[action="${endpoint}"]`)
      .getByRole("status");
    assert.equal(
      await status.count(),
      1,
      "Each ranking form needs an existing live region",
    );
    assert.equal(await status.textContent(), "");
    assert.equal(await status.getAttribute("aria-live"), "polite");
  }
  const candidateChoice = await pendingPlacement
    .getByRole("button", { name: "Prefer Later book", exact: true })
    .boundingBox();
  const opponentChoice = await pendingPlacement
    .getByRole("button", { name: "Prefer First book", exact: true })
    .boundingBox();
  assert.equal(
    candidateChoice.x < opponentChoice.x,
    true,
    "Desktop choices must be visible beside each other",
  );
  await recordScene("pending-comparison");
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
    await page
      .getByRole("status")
      .filter({ hasText: "We cannot confirm your save yet." })
      .isVisible(),
    true,
  );
  const recoveryHeading = page.getByRole("heading", {
    name: "Check your save",
    exact: true,
  });
  await recoveryHeading.waitFor();
  assert.equal(
    await page
      .getByText(
        "Check whether we saved your earlier action. This check can finish only that action.",
        { exact: true },
      )
      .isVisible(),
    false,
  );
  assert.equal(await page.title(), "Check your save · Book Smartz");
  await page.waitForLoadState("domcontentloaded");
  assert.equal(
    await recoveryHeading.evaluate(
      (heading) => document.activeElement === heading,
    ),
    true,
  );
  assert.match(
    await page.evaluate(() => getComputedStyle(document.body).fontFamily),
    /Source Sans 3/,
  );
  assert.equal(
    await page
      .getByRole("button", { name: "Check your save", exact: true })
      .count(),
    1,
  );
  const checkingStatus = page
    .locator('form[action="/recover"]')
    .getByRole("status");
  assert.equal(await checkingStatus.count(), 1);
  assert.equal(await checkingStatus.textContent(), "");
  assert.equal(await checkingStatus.getAttribute("aria-live"), "polite");
  await recordScene("uncertain-save");
  await page.getByRole("link", { name: "Return to your books" }).click();
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
  await page.getByRole("button", { name: "Check your save" }).click();
  await page.waitForURL("https://reader.example/ranking");
  assert.equal(
    await page
      .getByText(
        "You paused the comparisons for this book. Resume when you are ready.",
      )
      .isVisible(),
    true,
  );
  await page.reload();
  const oldResumeTab = await context.newPage();
  oldResumeTab.setDefaultTimeout(10000);
  await oldResumeTab.goto("https://reader.example/ranking");
  await oldResumeTab.waitForLoadState("domcontentloaded");
  await fixture("expire");
  const expired = page.waitForResponse(
    (response) =>
      response.url().endsWith("/resume") &&
      response.request().method() === "POST",
  );
  await page
    .getByRole("button", { name: "Resume comparisons", exact: true })
    .click();
  assert.equal((await expired).status(), 401);
  const signInRequired = page.getByRole("heading", {
    name: "Sign in to continue",
    exact: true,
  });
  await signInRequired.waitFor();
  await page.waitForLoadState("domcontentloaded");
  assert.equal(
    await signInRequired.evaluate(
      (heading) => document.activeElement === heading,
    ),
    true,
  );
  assert.equal(
    await page
      .getByRole("link", { name: "Sign in with Google", exact: true })
      .count(),
    1,
  );
  await recordScene("expired-session");
  await page.goto("https://reader.example/");
  await page.getByRole("link", { name: "Sign in with Google" }).click();
  await page.waitForURL("https://reader.example/ranking");
  assert.equal(
    await page
      .getByText(
        "You paused the comparisons for this book. Resume when you are ready.",
      )
      .isVisible(),
    true,
  );
  const staleSessionResponse = oldResumeTab.waitForResponse(
    (response) =>
      response.url().endsWith("/resume") &&
      response.request().method() === "POST",
  );
  await oldResumeTab
    .getByRole("button", { name: "Resume comparisons", exact: true })
    .click();
  assert.equal((await staleSessionResponse).status(), 403);
  const unavailable = oldResumeTab.getByRole("heading", {
    name: "Action unavailable",
    exact: true,
  });
  await unavailable.waitFor();
  await oldResumeTab.waitForLoadState("domcontentloaded");
  assert.equal(
    await unavailable.evaluate((heading) => document.activeElement === heading),
    true,
  );
  await recordScene("expired-form-csrf", oldResumeTab);
  await oldResumeTab.close();
  assert.equal(
    await page
      .getByText(
        "You paused the comparisons for this book. Resume when you are ready.",
        { exact: true },
      )
      .isVisible(),
    true,
  );
  await page
    .getByRole("button", { name: "Resume comparisons", exact: true })
    .click();
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
          status: form.querySelector('[role="status"]').textContent,
          busy: form.getAttribute("aria-busy"),
          saved: document.querySelectorAll("ol li").length,
          question: document.querySelector("[data-comparison-heading]")
            .textContent,
          focusOffStatus:
            document.activeElement !== form.querySelector('[role="status"]'),
        });
    });
  });
  let releasePreference;
  const preferenceGate = new Promise((resolve) => {
    releasePreference = resolve;
  });
  let arrivedPreference, failedPreference;
  const preferenceResponse = new Promise((resolve, reject) => {
    arrivedPreference = resolve;
    failedPreference = reject;
  });
  heldResponse = {
    path: "/answer",
    arrived: arrivedPreference,
    failed: failedPreference,
    release: preferenceGate,
  };
  const navigation = page.waitForNavigation();
  const preferencePress = prefer.press("Enter", { noWaitAfter: true });
  try {
    const [state, status] = await Promise.race([
      Promise.all([submitted, preferenceResponse]),
      delay(10000).then(() => {
        throw new Error("Held preference did not reach the server");
      }),
    ]);
    assert.deepEqual(state, {
      disabled: true,
      submitting: true,
      status: "Saving…",
      busy: "true",
      saved: 1,
      question: "Which book do you prefer?",
      focusOffStatus: true,
    });
    assert.equal(status, 303);
  } finally {
    heldResponse = null;
    releasePreference();
  }
  await preferencePress;
  await navigation;
  await page.waitForFunction(
    () => document.querySelectorAll("ol li").length === 2,
  );
  assert.equal(
    motions.length,
    completedMotions,
    "Keyboard placement, Pause, Resume, recovery, and reload must not replay pointer motion",
  );
  const titles = await page.locator("ol li a").allTextContents();
  assert.deepEqual(titles, ["Later book", "First book"]);
  await stale
    .getByRole("button", { name: "Prefer First book", exact: true })
    .click();
  const staleError = stale
    .getByRole("status")
    .filter({ hasText: "comparison changed" });
  await staleError.waitFor({ state: "visible", timeout: 5000 });
  await stale.waitForLoadState("domcontentloaded");
  assert.equal(
    await staleError.evaluate((error) => document.activeElement === error),
    true,
  );
  assert.equal(
    (
      await stale
        .getByRole("heading", { name: "Your books", exact: true })
        .boundingBox()
    ).x <
      (
        await stale
          .getByRole("heading", {
            name: "Review your books",
            exact: true,
          })
          .boundingBox()
      ).x,
    true,
  );
  await recordScene("stale-rejection", stale);
  await page.reload();
  assert.deepEqual(await page.locator("ol li a").allTextContents(), titles);
  await page.waitForTimeout(1100);
  await page
    .getByRole("textbox", { name: "Title or author" })
    .fill("Unavailable");
  await page.getByRole("button", { name: "Search books", exact: true }).click();
  await page
    .getByRole("button", { name: "Search books", exact: true })
    .waitFor({ state: "visible", timeout: 5000 });
  assert.equal(
    await page
      .getByText("Search is unavailable. Try again.", { exact: true })
      .isVisible(),
    true,
  );
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
    await page
      .getByRole("button", { name: "Search books", exact: true })
      .click();
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
    .getByRole("button", { name: "Select this book", exact: true })
    .click();
  await page
    .getByRole("heading", { name: "Selected book", exact: true })
    .waitFor();
  await recordScene("selected-long-title");
  await page.getByRole("link", { name: "Return to your books" }).click();
  await deliberateSearch("No matches");
  await page
    .getByText("We found no matching books. Try another title or author.", {
      exact: true,
    })
    .waitFor();
  await recordScene("empty-results");
  assert.equal(
    await page
      .getByRole("button", { name: "Select this book", exact: true })
      .count(),
    0,
  );
  async function addWork(title) {
    await page.goto("https://reader.example/ranking");
    await deliberateSearch(title);
    await page
      .getByRole("button", { name: "Select this book", exact: true })
      .click();
    await page.waitForLoadState("domcontentloaded");
    await page
      .getByRole("button", { name: "Add to your books", exact: true })
      .click();
    await page.waitForURL("https://reader.example/ranking");
    await page.waitForLoadState("domcontentloaded");
  }
  async function completedMotion(title, after = 0) {
    for (let attempt = 0; attempt < 150; attempt++) {
      const record = motions
        .slice(after)
        .find(
          (motion) => motion.type === "transitionend" && motion.title === title,
        );
      if (record) return record;
      await delay(20);
    }
    throw new Error(`No completed highlight for ${title}`);
  }
  await page.setViewportSize({ width: 1440, height: 900 });
  await addWork("Third book");
  await page.waitForLoadState("load");
  assert.equal(
    coverRequests.includes(
      "https://covers.openlibrary.org/b/id/123-M.jpg?default=false",
    ),
    true,
  );
  const thirdSummary = page.getByRole("article").filter({
    has: page.getByRole("link", { name: "Third book", exact: true }),
  });
  assert.equal(
    await thirdSummary
      .getByLabel("Example cover for this book", { exact: true })
      .isVisible(),
    true,
  );
  assert.equal(
    await thirdSummary
      .getByRole("img", { name: "Cover unavailable", exact: true })
      .count(),
    1,
    "An unavailable remote cover exposes one accessible placeholder",
  );
  assert.equal(await page.locator("ol li").count(), 2);
  await page
    .getByRole("button", { name: "Prefer Later book", exact: true })
    .click();

  await page.getByText("We saved your choice.", { exact: true }).waitFor();
  await recordScene("saved-preference-progress");
  await page
    .getByRole("button", { name: "Pause for now", exact: true })
    .click();
  await page.getByText("We paused the comparisons.", { exact: true }).waitFor();
  await page
    .getByText(
      "You paused the comparisons for this book. Resume when you are ready.",
      { exact: true },
    )
    .waitFor();
  await recordScene("paused-placement");
  await page.reload();
  await page
    .getByRole("button", { name: "Resume comparisons", exact: true })
    .waitFor();
  await page
    .getByRole("button", { name: "Sign out of this browser", exact: true })
    .click();
  await page
    .getByRole("link", { name: "Sign in with Google", exact: true })
    .click();
  await page.waitForURL("https://reader.example/ranking");
  await page
    .getByRole("button", { name: "Resume comparisons", exact: true })
    .waitFor();
  assert.equal(
    await page
      .getByRole("button", { name: "Prefer Third book", exact: true })
      .count(),
    0,
  );
  const resumeStatus = page
    .locator('form[action="/resume"]')
    .getByRole("status");
  assert.equal(await resumeStatus.count(), 1);
  assert.equal(await resumeStatus.textContent(), "");
  await page
    .getByRole("button", { name: "Resume comparisons", exact: true })
    .click();
  await page
    .getByText("We resumed the comparisons.", {
      exact: true,
    })
    .waitFor();
  await page
    .getByRole("heading", { name: "Which book do you prefer?", exact: true })
    .waitFor();
  await page.waitForLoadState("domcontentloaded");
  const beforeThirdPlacement = motions.length;
  await page
    .getByRole("button", { name: "Prefer Third book", exact: true })
    .click();
  await page
    .getByRole("status")
    .filter({ hasText: "We saved this book at position 2." })
    .waitFor();
  await completedMotion("Third book", beforeThirdPlacement);
  assert.deepEqual(
    motions
      .slice(beforeThirdPlacement)
      .map((motion) => [motion.type, motion.title]),
    [
      ["transitionrun", "Third book"],
      ["transitionend", "Third book"],
    ],
  );
  assert.equal(await page.locator("ol li").count(), 3);
  await recordScene("pointer-completed-placement");
  const thirdRow = page.getByRole("listitem").filter({ hasText: "Third book" });
  const highlightedThird = await thirdRow.screenshot();
  const beforeReload = motions.length;
  await page.reload();
  const cleanThird = await thirdRow.screenshot();
  assert.equal(highlightedThird.equals(cleanThird), false);
  await page.reload();
  assert.equal((await thirdRow.screenshot()).equals(cleanThird), true);
  assert.equal(
    motions.length,
    beforeReload,
    "Reload must not replay the completion transition",
  );
  await page.emulateMedia({ reducedMotion: "reduce" });
  const beforeReduced = motions.length;
  await addWork("Reduced book");
  for (
    let choice = 0;
    choice < 6 &&
    (await page
      .getByRole("button", { name: "Prefer Reduced book", exact: true })
      .count());
    choice++
  ) {
    await page
      .getByRole("button", { name: "Prefer Reduced book", exact: true })
      .click();
    await page.waitForLoadState("domcontentloaded");
  }
  await page
    .getByRole("status")
    .filter({ hasText: "We saved this book at position 1." })
    .waitFor();
  assert.equal(await page.locator("ol li").count(), 4);
  const reducedHighlight = await page
    .getByRole("listitem")
    .filter({ hasText: "Reduced book" })
    .evaluate((row) => ({
      opacity: getComputedStyle(row, "::before").opacity,
      duration: getComputedStyle(row, "::before").transitionDuration,
    }));
  assert.deepEqual(reducedHighlight, { opacity: "0.12", duration: "0s" });
  assert.equal(
    motions.length,
    beforeReduced,
    "Reduced motion must show the final highlight immediately",
  );
  await recordScene("reduced-motion-completion");
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await addWork("Interrupted book");
  const beforeInterruption = motions.length;
  const searchesBeforeInterruption = searchRequests;
  let editingDuringHighlight;
  observeInterruption = (record) => {
    if (
      record.type === "transitionrun" &&
      record.title === "Interrupted book"
    ) {
      editingDuringHighlight = page
        .getByRole("textbox", { name: "Title or author" })
        .fill("Available during highlight");
      return editingDuringHighlight;
    }
  };
  for (
    let choice = 0;
    choice < 8 &&
    (await page
      .getByRole("button", { name: "Prefer Interrupted book", exact: true })
      .count());
    choice++
  ) {
    await page
      .getByRole("button", { name: "Prefer Interrupted book", exact: true })
      .click();
    await page.waitForLoadState("domcontentloaded");
  }
  await completedMotion("Interrupted book", beforeInterruption);
  assert.ok(editingDuringHighlight, "Highlight must allow immediate input");
  await editingDuringHighlight;
  const interruption = motions
    .slice(beforeInterruption)
    .find((record) => record.type === "input");
  assert.deepEqual(interruption, {
    type: "input",
    trusted: true,
    running: true,
  });
  assert.deepEqual(
    motions
      .slice(beforeInterruption)
      .filter((record) => record.type !== "input")
      .map((record) => [record.type, record.title]),
    [
      ["transitionrun", "Interrupted book"],
      ["transitionend", "Interrupted book"],
    ],
  );
  assert.equal(
    searchRequests,
    searchesBeforeInterruption,
    "Editing during a highlight must not submit a search",
  );
  assert.equal(
    await page.getByRole("textbox", { name: "Title or author" }).inputValue(),
    "Available during highlight",
  );
  await recordScene("interrupted-highlight");
  // Respect the real reader budget before the eleventh deliberate search.
  await delay(Math.max(0, 65000 - (Date.now() - firstSearchAt)));
  await addWork("Recovered book");
  await page
    .getByRole("button", { name: "Prefer Recovered book", exact: true })
    .click();
  await page.waitForLoadState("domcontentloaded");
  await page.getByText("We saved your choice.", { exact: true }).waitFor();
  const beforeRecoveredMotion = motions.length;
  await fixture("reject");
  await page
    .getByRole("button", { name: "Prefer Recovered book", exact: true })
    .click();
  await page
    .getByRole("heading", { name: "Check your save", exact: true })
    .waitFor();
  await page.waitForLoadState("domcontentloaded");
  await recordScene("unresolved-completion");
  await fixture("allow");
  const replayRecovery = await context.newPage();
  replayRecovery.setDefaultTimeout(10000);
  const recoveriesBeforeRead = mutationRequests.get("/recover") ?? 0;
  await replayRecovery.goto("https://reader.example/ranking");
  await replayRecovery.reload();
  assert.equal(await replayRecovery.locator("ol li").count(), 5);
  assert.equal(
    await replayRecovery
      .getByRole("button", { name: "Prefer Recovered book", exact: true })
      .isDisabled(),
    true,
  );
  assert.equal(
    mutationRequests.get("/recover") ?? 0,
    recoveriesBeforeRead,
    "Reads must not submit recovery",
  );
  let captureRecovery;
  const submittedRecovery = new Promise((resolve) => {
    captureRecovery = resolve;
  });
  await page.exposeFunction("captureRecoverySubmission", (state) =>
    captureRecovery(state),
  );
  await page.evaluate(() => {
    let captured = false;
    let duplicatePrevented = false;
    document.addEventListener("submit", (event) => {
      const form = event.target;
      if (form.getAttribute("action") !== "/recover") return;
      if (captured) {
        duplicatePrevented = event.defaultPrevented;
        return;
      }
      captured = true;
      form.dispatchEvent(
        new window.SubmitEvent("submit", { bubbles: true, cancelable: true }),
      );
      const status = form.querySelector('[role="status"]');
      window.captureRecoverySubmission({
        disabled: [...form.querySelectorAll("button")].every(
          (button) => button.disabled,
        ),
        busy: form.getAttribute("aria-busy"),
        status: status.textContent,
        focusOffStatus: document.activeElement !== status,
        success: [...document.querySelectorAll('[role="status"]')].some(
          (status) =>
            /We saved this book at position|We confirmed your save/.test(
              status.textContent,
            ),
        ),
        duplicatePrevented,
      });
    });
  });
  let releaseRecovery;
  const recoveryGate = new Promise((resolve) => {
    releaseRecovery = resolve;
  });
  let arrivedRecovery, failedRecovery;
  const recoveryResponse = new Promise((resolve, reject) => {
    arrivedRecovery = resolve;
    failedRecovery = reject;
  });
  heldResponse = {
    path: "/recover",
    arrived: arrivedRecovery,
    failed: failedRecovery,
    release: recoveryGate,
  };
  const recoveryClick = page
    .getByRole("button", { name: "Check your save", exact: true })
    .click({ noWaitAfter: true });
  try {
    const [state, status] = await Promise.race([
      Promise.all([submittedRecovery, recoveryResponse]),
      delay(10000).then(() => {
        throw new Error("Held recovery did not reach the server");
      }),
    ]);
    assert.deepEqual(state, {
      disabled: true,
      busy: "true",
      status: "Saving…",
      focusOffStatus: true,
      success: false,
      duplicatePrevented: true,
    });
    assert.equal(status, 303);
    assert.equal(mutationRequests.get("/recover"), recoveriesBeforeRead + 1);
  } finally {
    heldResponse = null;
    releaseRecovery();
  }
  await recoveryClick;
  await page.waitForURL("https://reader.example/ranking");
  await page.waitForLoadState("domcontentloaded");
  await page.getByText("We confirmed your save.", { exact: true }).waitFor();
  const recoveredTitles = await page.locator("ol li a").allTextContents();
  assert.equal(recoveredTitles.length, 6);
  assert.equal(recoveredTitles[0], "Recovered book");
  await recordScene("recovered-completion");
  assert.equal(
    motions.length,
    beforeRecoveredMotion,
    "Recovery must not replay completion decoration",
  );
  await replayRecovery
    .getByRole("button", { name: "Check your save", exact: true })
    .click();
  await replayRecovery
    .getByText("We confirmed your save.", { exact: true })
    .waitFor();
  assert.deepEqual(
    await replayRecovery.locator("ol li a").allTextContents(),
    recoveredTitles,
  );
  await recordScene("replayed-original-save", replayRecovery);
  assert.equal(
    motions.length,
    beforeRecoveredMotion,
    "Repeated recovery must not replay completion decoration",
  );
  await replayRecovery.close();
  await fixture("native-reader");
  const recoveryJourney = await browser.newContext({
    javaScriptEnabled: true,
    ignoreHTTPSErrors: true,
    viewport: { width: 320, height: 900 },
  });
  await recoveryJourney.route("https://covers.openlibrary.org/**", (route) =>
    route.abort(),
  );
  const recoveryReaderPage = await recoveryJourney.newPage();
  recoveryReaderPage.setDefaultTimeout(10000);
  recoveryReaderPage.setDefaultNavigationTimeout(10000);
  let recoveryRankingRequests = 0;
  recoveryReaderPage.on("request", (request) => {
    if (
      request.method() === "POST" &&
      ["/add", "/answer", "/pause", "/resume", "/recover"].includes(
        new URL(request.url()).pathname,
      )
    )
      recoveryRankingRequests++;
  });
  await recoveryReaderPage.goto("https://reader.example/");
  await recoveryReaderPage
    .getByRole("link", { name: "Sign in with Google", exact: true })
    .click();
  await recoveryReaderPage.waitForURL("https://reader.example/ranking");
  assert.equal(await recoveryReaderPage.locator("ol li").count(), 0);
  assert.equal(recoveryRankingRequests, 0);
  await recordScene("recovery-empty-ranking", recoveryReaderPage);
  async function recoverySelection(query) {
    await recoveryReaderPage
      .getByRole("textbox", { name: "Title or author" })
      .fill(query);
    await recoveryReaderPage.waitForTimeout(1100);
    await recoveryReaderPage
      .getByRole("button", { name: "Search books", exact: true })
      .click();
    await recoveryReaderPage
      .getByRole("button", { name: "Select this book", exact: true })
      .click();
    await recoveryReaderPage
      .getByRole("heading", { name: "Selected book", exact: true })
      .waitFor();
  }
  await recoverySelection("Native book");
  await recoveryReaderPage
    .getByRole("button", { name: "Add to your books", exact: true })
    .click();
  await recoveryReaderPage
    .getByText("We saved this book at position 1.", { exact: true })
    .waitFor();
  assert.deepEqual(
    await recoveryReaderPage.locator("ol li a").allTextContents(),
    ["Native book"],
  );
  await recordScene("recovery-first-placement", recoveryReaderPage);
  await recoverySelection("Long title");
  await recoveryReaderPage
    .getByText("Author unavailable", { exact: true })
    .waitFor();
  await recordScene("recovery-selected-long-title", recoveryReaderPage);
  await recoveryReaderPage
    .getByRole("button", { name: "Add to your books", exact: true })
    .click();
  await recoveryReaderPage
    .getByRole("button", { name: "Prefer Native book", exact: true })
    .waitFor();
  assert.equal(await recoveryReaderPage.locator("ol li").count(), 1);
  await recordScene("recovery-long-comparison", recoveryReaderPage);
  await fixture("reject");
  await recoveryReaderPage
    .getByRole("button", { name: "Pause for now", exact: true })
    .click();
  await recoveryReaderPage
    .getByRole("heading", { name: "Check your save", exact: true })
    .waitFor();
  await recordScene("recovery-uncertain-save", recoveryReaderPage);
  await fixture("allow");
  const beforeRecoveryReads = recoveryRankingRequests;
  await recoveryReaderPage
    .getByRole("link", { name: "Return to your books", exact: true })
    .click();
  await recoveryReaderPage.reload();
  assert.equal(await recoveryReaderPage.locator("ol li").count(), 1);
  assert.equal(
    await recoveryReaderPage
      .getByText(
        "You paused the comparisons for this book. Resume when you are ready.",
        { exact: true },
      )
      .count(),
    0,
  );
  assert.equal(
    await recoveryReaderPage
      .getByRole("button", { name: "Pause for now", exact: true })
      .isDisabled(),
    true,
  );
  await recoveryReaderPage
    .getByRole("button", { name: "Sign out of this browser", exact: true })
    .click();
  await recoveryReaderPage
    .getByRole("link", { name: "Sign in with Google", exact: true })
    .click();
  await recoveryReaderPage.waitForURL("https://reader.example/ranking");
  assert.equal(
    recoveryRankingRequests,
    beforeRecoveryReads,
    "Read, reload, and sign-in must not retry a ranking action",
  );
  assert.equal(
    await recoveryReaderPage
      .getByText(
        "You paused the comparisons for this book. Resume when you are ready.",
        { exact: true },
      )
      .count(),
    0,
  );
  assert.equal(
    await recoveryReaderPage
      .getByRole("button", { name: "Pause for now", exact: true })
      .isDisabled(),
    true,
  );
  await recordScene("recovery-retained-original-action", recoveryReaderPage);
  await recoveryReaderPage
    .getByRole("button", { name: "Check your save", exact: true })
    .click();
  await recoveryReaderPage
    .getByText("We confirmed your save.", { exact: true })
    .waitFor();
  await recoveryReaderPage
    .getByText(
      "You paused the comparisons for this book. Resume when you are ready.",
      { exact: true },
    )
    .waitFor();
  await recordScene("recovery-recovered-pause", recoveryReaderPage);
  await recoveryReaderPage
    .getByRole("button", { name: "Resume comparisons", exact: true })
    .click();
  const recoveryPreference = recoveryReaderPage.getByRole("button", {
    name: "Prefer Native book",
    exact: true,
  });
  await recoveryPreference.waitFor();
  for (
    let step = 0;
    step < 30 &&
    !(await recoveryPreference.evaluate(
      (button) => document.activeElement === button,
    ));
    step++
  )
    await recoveryReaderPage.keyboard.press("Tab");
  assert.equal(
    await recoveryPreference.evaluate(
      (button) =>
        document.activeElement === button &&
        getComputedStyle(button).outlineStyle !== "none",
    ),
    true,
  );
  const recoveryFinish = recoveryReaderPage.waitForNavigation();
  await recoveryReaderPage.keyboard.press("Enter");
  await recoveryFinish;
  await recoveryReaderPage
    .getByText("We saved this book at position 2.", { exact: true })
    .waitFor();
  const recoveryTitles = await recoveryReaderPage
    .locator("ol li a")
    .allTextContents();
  assert.equal(recoveryTitles.length, 2);
  assert.equal(recoveryTitles[0], "Native book");
  assert.match(recoveryTitles[1], /^A long title/);
  assert.equal(recoveryRankingRequests, beforeRecoveryReads + 3);
  await recordScene("recovery-completed-placement", recoveryReaderPage);
  await recoveryJourney.close();
  if (evidence)
    writeFileSync(
      join(evidence, "motion-evidence.json"),
      JSON.stringify(
        {
          commit: process.env.BOOK_SMARTZ_ACCEPTANCE_COMMIT || "working-tree",
          engine: "Chromium",
          version: browser.version(),
          motions,
        },
        null,
        2,
      ),
    );
  console.log("Browser version:", browser.version());
  console.log(
    "Browser assertions passed: JavaScript reader actions, disabled-JavaScript notice, explicit recovery, held responses, motion, contrast, and responsive layout.",
  );
  lifecycle.record("assertions", "complete");
}

await runJourney(lifecycle, journey, {
  controls() {
    controls.close();
    process.stdin.destroy();
  },
  async browser() {
    await browser?.close();
    if (stalledPhase === "browser_shutdown")
      await new Promise(() => {
        /* Retain the phase until its cleanup deadline. */
      });
  },
  async proxy() {
    for (const socket of sockets) socket.destroy();
    await Promise.all([
      new Promise((resolve) => proxy.close(resolve)),
      new Promise((resolve) => transport.close(resolve)),
    ]);
    if (stalledPhase === "proxy_shutdown")
      await new Promise(() => {
        /* Retain the phase until its cleanup deadline. */
      });
  },
});
