import assert from "node:assert/strict";
import { presentation, withPage } from "./harness.mjs";

async function tabStops(page, steps) {
  const stops = [];
  for (let index = 0; index < steps; index++) {
    await page.keyboard.press("Tab");
    stops.push(
      await page.evaluate(() => ({
        href: document.activeElement?.getAttribute("href"),
        inHero: Boolean(document.activeElement?.closest("home-hero-animation")),
      })),
    );
  }
  return stops;
}

await withPage(
  { route: "/tests/hero/", clock: true, initialPaintSelector: ".hero-actions" },
  async (page) => {
    await page.clock.runFor(16);
    const firstPaint = await page.evaluate(() => window.__initialPaintSamples);
    assert.ok(firstPaint.length > 0, "first rendered frame was sampled");
    assert.ok(
      firstPaint.every(
        (sample) => sample.visibility === "hidden" || sample.opacity === 0,
      ),
      "hero actions have no visible initial paint",
    );
    const details = [
      ".eyebrow",
      ".hero-description",
      ".hero-actions",
      ".hero-socials",
      ".hero-note",
      ".portrait-note",
    ];
    await page.evaluate(() => document.fonts.ready);
    assert.equal(
      (await presentation(page, ".ink-underline > svg")).visibility,
      "hidden",
      "underline is hidden before heading writing",
    );
    const headingStart = await presentation(page, "h1");
    const initial = await Promise.all(
      details.map((selector) => presentation(page, selector)),
    );
    for (const state of initial)
      assert.ok(
        state.opacity === 0 || state.visibility === "hidden",
        "hero detail is initially hidden",
      );
    const pendingStops = await tabStops(page, 14);
    assert.ok(
      pendingStops.some((stop) => stop.href === "/blog/"),
      "keyboard traverses preceding navigation",
    );
    assert.ok(
      pendingStops.every((stop) => !stop.inHero),
      "pending hero controls are untabbable throughout keyboard traversal",
    );
    await page.clock.runFor(1484);
    for (const selector of details) {
      const state = await presentation(page, selector);
      assert.ok(
        state.opacity === 0 || state.visibility === "hidden",
        `${selector} stays hidden while heading writes`,
      );
    }
    assert.ok((await page.locator("h1 .handwriting-word").count()) > 0);
    assert.equal(
      (await presentation(page, ".ink-underline > svg")).visibility,
      "hidden",
      "underline stays hidden while the headline writes",
    );
    await page.clock.runFor(1200);
    for (const selector of details) {
      const state = await presentation(page, selector);
      assert.ok(
        state.opacity === 0 || state.visibility === "hidden",
        `${selector} stays hidden during underline drawing`,
      );
    }
    assert.equal(
      (await presentation(page, ".ink-underline > svg")).visibility,
      "visible",
      "underline becomes visible for its drawing phase",
    );
    const underline = await page
      .locator(".ink-underline > svg path")
      .evaluate((path) => ({
        dasharray: getComputedStyle(path).strokeDasharray,
        dashoffset: getComputedStyle(path).strokeDashoffset,
      }));
    assert.notEqual(
      underline.dasharray,
      "none",
      "underline drawing is active after heading writing",
    );
    let revealStarted = false;
    for (let frame = 0; frame < 100; frame++) {
      await page.clock.runFor(16);
      const actions = await presentation(page, ".hero-actions");
      if (actions.visibility !== "visible") continue;
      revealStarted = true;
      for (const selector of [".hero-actions", ".hero-socials"]) {
        const state = await presentation(page, selector);
        assert.equal(
          state.opacity,
          0,
          `${selector} is in the transparent stagger interval`,
        );
        const acceptedFocus = await page
          .locator(`${selector} a`)
          .evaluateAll((links) =>
            links.some((link) => {
              link.focus({ preventScroll: true });
              return document.activeElement === link;
            }),
          );
        assert.equal(
          acceptedFocus,
          false,
          `${selector} rejects focus while transparent`,
        );
      }
      break;
    }
    assert.equal(
      revealStarted,
      true,
      "observed the start of the actual detail reveal",
    );
    await page.clock.runFor(2800);
    for (let index = 0; index < details.length; index++) {
      const state = await presentation(page, details[index]);
      assert.equal(
        state.visibility,
        "visible",
        `${details[index]} visible after underline`,
      );
      assert.ok(
        state.opacity > 0.99,
        `${details[index]} opaque after underline`,
      );
      assert.ok(
        Math.abs(state.box.width - initial[index].box.width) < 1,
        `${details[index]} retains its reserved width`,
      );
      assert.ok(
        Math.abs(state.box.height - initial[index].box.height) < 1,
        `${details[index]} retains its reserved height`,
      );
    }
    assert.equal(
      (await presentation(page, ".ink-underline > svg")).visibility,
      "visible",
      "completed underline remains visible",
    );
    const headingEnd = await presentation(page, "h1");
    for (const dimension of ["x", "y", "width", "height"]) {
      assert.ok(
        Math.abs(headingEnd.box[dimension] - headingStart.box[dimension]) < 1,
        `heading retains its ${dimension} bound`,
      );
    }
    const completedStops = await tabStops(page, 18);
    for (const href of [
      "mailto:hello@example.com",
      "/about/",
      "https://github.com/tacascer",
    ]) {
      assert.ok(
        completedStops.some((stop) => stop.inHero && stop.href === href),
        `${href} becomes keyboard reachable after reveal`,
      );
    }
  },
);

for (const options of [
  { reducedMotion: "reduce" },
  { javaScriptEnabled: false },
  { fontMode: "blocked" },
]) {
  await withPage({ route: "/tests/hero/", ...options }, async (page) => {
    await page.waitForTimeout(100);
    const detail = await presentation(page, ".hero-actions");
    assert.equal(
      detail.visibility,
      "visible",
      `${JSON.stringify(options)} keeps static controls visible`,
    );
    assert.ok(detail.opacity > 0.99);
    assert.equal(
      (await presentation(page, ".ink-underline > svg")).visibility,
      "visible",
      "static fallback keeps the underline visible",
    );
    if (options.reducedMotion === "reduce") {
      const art = await page.locator(".hero-art").evaluate((element) => ({
        opacity: getComputedStyle(element).opacity,
        translate: getComputedStyle(element).translate,
      }));
      assert.equal(
        art.opacity,
        "1",
        "reduced motion does not animate the illustration",
      );
      assert.equal(art.translate, "none");
    }
  });
}

await withPage(
  { route: "/tests/hero/", blockAnimationModule: true },
  async (page) => {
    await page.waitForTimeout(2200);
    assert.equal(
      (await presentation(page, ".hero-actions")).visibility,
      "visible",
      "module failure restores static controls within two seconds",
    );
  },
);

for (const change of [
  async (page) => page.emulateMedia({ reducedMotion: "reduce" }),
  async (page) => page.setViewportSize({ width: 900, height: 800 }),
]) {
  await withPage({ route: "/tests/hero/" }, async (page) => {
    await page.locator("home-hero-animation[data-animation-pending]").waitFor();
    await change(page);
    await page.waitForFunction(
      () =>
        !document
          .querySelector("home-hero-animation")
          ?.hasAttribute("data-animation-pending"),
      null,
      { timeout: 1000 },
    );
    assert.equal(
      (await presentation(page, ".hero-actions")).visibility,
      "visible",
      "cancellation restores controls",
    );
    assert.equal(
      await page.locator("home-hero-animation[data-animation-pending]").count(),
      0,
    );
    assert.equal(
      (await presentation(page, ".ink-underline > svg")).visibility,
      "visible",
      "cancellation restores the underline",
    );
    const art = await page.locator(".hero-art").evaluate((element) => ({
      opacity: getComputedStyle(element).opacity,
      translate: getComputedStyle(element).translate,
    }));
    assert.equal(art.opacity, "1", "cancellation restores illustration");
    assert.equal(art.translate, "none");
  });
}

await withPage({ route: "/tests/hero/", fontMode: "delayed" }, async (page) => {
  const root = await page.locator("home-hero-animation").elementHandle();
  await root.evaluate((element) => element.remove());
  await page.waitForTimeout(2800);
  assert.equal(
    await root.evaluate((element) =>
      element.hasAttribute("data-animation-pending"),
    ),
    false,
    "disconnected root is restored after delayed font response",
  );
});

await withPage({ route: "/tests/hero/" }, async (page) => {
  await page.locator("home-hero-animation[data-animation-pending]").waitFor();
  const root = await page.locator("home-hero-animation").elementHandle();
  await root.evaluate((element) => element.remove());
  await page.waitForTimeout(500);
  assert.equal(
    await root.evaluate((element) =>
      element.hasAttribute("data-animation-pending"),
    ),
    false,
    "disconnect cancels writing",
  );
});

await withPage({ route: "/tests/lifecycle/" }, async (page) => {
  const snapshot = await page.evaluate(() => ({
    roots: [...document.querySelectorAll(".lifecycle-fixture")].map((root) =>
      root.outerHTML.slice(0, 160),
    ),
    fonts: [...document.fonts].map((font) => ({
      family: font.family,
      status: font.status,
    })),
  }));
  assert.equal(
    await page.locator(".lifecycle-fixture[data-animation-pending]").count(),
    2,
    JSON.stringify(snapshot),
  );
  assert.equal(
    (await presentation(page, ".lifecycle-fixture:nth-of-type(3) a"))
      .visibility,
    "visible",
    "unsupported glyph falls back to static detail",
  );
  await page.waitForTimeout(700);
  assert.equal(
    (await presentation(page, ".lifecycle-fixture:nth-of-type(1) a"))
      .visibility,
    "hidden",
  );
  assert.equal(
    (await presentation(page, ".lifecycle-fixture:nth-of-type(2) a"))
      .visibility,
    "hidden",
  );
  await page.waitForTimeout(1100);
  assert.equal(
    (await presentation(page, ".lifecycle-fixture:nth-of-type(1) a"))
      .visibility,
    "visible",
    "1200ms handwriting releases its own detail",
  );
  assert.equal(
    (await presentation(page, ".lifecycle-fixture:nth-of-type(2) a"))
      .visibility,
    "hidden",
    "2600ms handwriting still owns its detail",
  );
  await page.waitForTimeout(1900);
  assert.equal(
    (await presentation(page, ".lifecycle-fixture:nth-of-type(2) a"))
      .visibility,
    "visible",
    "longer handwriting releases after actual completion",
  );
});
