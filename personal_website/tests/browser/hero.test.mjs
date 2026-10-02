import assert from "node:assert/strict";
import { presentation, withPage } from "./harness.mjs";

await withPage({ route: "/tests/hero/", clock: true }, async (page) => {
  const details = [
    ".eyebrow",
    ".hero-description",
    ".hero-actions",
    ".hero-socials",
    ".hero-note",
    ".portrait-note",
  ];
  await page.evaluate(() => document.fonts.ready);
  const initial = await Promise.all(
    details.map((selector) => presentation(page, selector)),
  );
  for (const state of initial)
    assert.ok(
      state.opacity === 0 || state.visibility === "hidden",
      "hero detail is initially hidden",
    );
  await page.keyboard.press("Tab");
  const focused = await page.evaluate(
    () => document.activeElement?.closest(".home-hero") !== null,
  );
  assert.equal(focused, false, "pending hero controls are untabbable");
  await page.clock.runFor(1500);
  for (const selector of details) {
    const state = await presentation(page, selector);
    assert.ok(
      state.opacity === 0 || state.visibility === "hidden",
      `${selector} stays hidden while heading writes`,
    );
  }
  await page.clock.runFor(2800);
  for (let index = 0; index < details.length; index++) {
    const state = await presentation(page, details[index]);
    assert.equal(
      state.visibility,
      "visible",
      `${details[index]} visible after underline`,
    );
    assert.ok(state.opacity > 0.99, `${details[index]} opaque after underline`);
    assert.ok(
      Math.abs(state.box.width - initial[index].box.width) < 1,
      `${details[index]} retains its reserved width`,
    );
    assert.ok(
      Math.abs(state.box.height - initial[index].box.height) < 1,
      `${details[index]} retains its reserved height`,
    );
  }
});

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
    assert.equal(
      (await presentation(page, ".hero-actions")).visibility,
      "visible",
      "cancellation restores controls",
    );
    assert.equal(
      await page.locator("home-hero-animation[data-animation-pending]").count(),
      0,
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
