/* global process, URL, getComputedStyle, document, console, window */
import { createInterface } from "node:readline";
import assert from "node:assert/strict";
import { chromium } from "playwright";
import { readFileSync } from "node:fs";
import { createServer as httpServer, request } from "node:http";
import { createServer as httpsServer } from "node:https";
import { connect } from "node:net";

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
const transport = httpsServer(
  { key: readFileSync(process.argv[3]), cert: readFileSync(process.argv[4]) },
  (incoming, outgoing) => {
    if (incoming.method === "POST")
      assert.equal(
        incoming.headers.origin,
        "https://reader.example",
        "Native forms must retain the canonical Origin",
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
      (response) => {
        outgoing.writeHead(response.statusCode, response.headers);
        response.pipe(outgoing);
      },
    );
    upstream.on("error", () => {
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
  process.env.CHROMIUM_EXECUTABLE || process.env.CHROME_BIN;
assert.ok(
  executablePath,
  "Use the Nix shell or set CHROMIUM_EXECUTABLE/CHROME_BIN",
);
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
    viewport: { width: 1280, height: 900 },
    ignoreHTTPSErrors: true,
  });
  await context.route("https://covers.openlibrary.org/**", (route) =>
    route.abort(),
  );
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  page.setDefaultNavigationTimeout(10000);
  await page.goto("https://reader.example/");
  await page.getByRole("link", { name: "Sign in with Google" }).click();
  await page.waitForURL("https://reader.example/ranking");
  await page
    .getByRole("textbox", { name: "Title or author" })
    .fill("First book");
  await page.getByRole("button", { name: "Search books", exact: true }).click();
  assert.equal(await page.getByAltText("Cover unavailable").count(), 1);
  await page.getByRole("button", { name: "Select this work" }).click();
  await page.getByRole("button", { name: "Add to your ranking" }).click();
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
  assert.equal(
    await stale
      .getByRole("status")
      .filter({ hasText: "comparison changed" })
      .isVisible(),
    true,
  );
  await page.reload();
  assert.deepEqual(await page.locator("ol li a").allTextContents(), titles);
  await page.waitForTimeout(1100);
  await page
    .getByRole("textbox", { name: "Title or author" })
    .fill("Unavailable");
  await page.getByRole("button", { name: "Search books", exact: true }).click();
  assert.equal(
    await page.getByRole("button", { name: /Retry/ }).isVisible(),
    true,
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
