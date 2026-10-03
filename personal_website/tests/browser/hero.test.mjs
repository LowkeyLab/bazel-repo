import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import sharp from "sharp";
import { presentation, withPage } from "./harness.mjs";

// Revealing the same HTML must preserve the final glyphs and layout, including
// mobile line wrapping. Mask compositing differs by up to two colour levels
// in the pinned CI Chromium, even when glyphs and layout are unchanged.
for (const viewport of [
  { width: 1280, height: 900 },
  { width: 390, height: 844 },
]) {
  await withPage(
    { route: "/tests/hero/", clock: true, viewport },
    async (page) => {
      await page.evaluate(() => document.fonts.ready);
      await page.evaluate(() =>
        customElements.whenDefined("home-hero-animation"),
      );
      // Mount is scheduled with setTimeout(0); explicitly advance the paused clock.
      await page.clock.runFor(1);
      await page
        .locator("h1 .handwriting-overlay")
        .first()
        .waitFor({ state: "attached" });
      const screenshotOptions = {
        style: ".ink-underline > svg { visibility: hidden !important; }",
      };
      const beforeWriting = await page
        .locator("h1")
        .screenshot(screenshotOptions);
      await page.clock.runFor(1200);
      const duringWriting = await page
        .locator("h1")
        .screenshot(screenshotOptions);
      assert.ok(
        !beforeWriting.equals(duringWriting),
        "handwriting must visibly reveal letters",
      );
      // Finish every glyph through the real mask, without removing the mask itself.
      assert.ok(
        (await page.locator("h1 .handwriting-overlay").count()) > 0,
        "animation must still be active at comparison",
      );
      assert.ok(
        (await page.locator("h1 mask rect").count()) > 0,
        "completed-mask comparison requires glyphs",
      );
      await page.locator("h1 mask rect").evaluateAll((finishes) => {
        for (const finish of finishes)
          finish.style.setProperty("opacity", "1", "important");
      });
      const animated = await page.locator("h1").screenshot(screenshotOptions);
      await page.clock.runFor(5000);
      assert.equal(
        await page.locator("h1 .handwriting-overlay").count(),
        0,
        "final screenshot must follow animation cleanup",
      );
      const final = await page.locator("h1").screenshot(screenshotOptions);
      const before = await sharp(animated)
        .raw()
        .toBuffer({ resolveWithObject: true });
      const after = await sharp(final)
        .raw()
        .toBuffer({ resolveWithObject: true });
      assert.deepEqual(
        before.info,
        after.info,
        "heading dimensions change at completion",
      );
      const changed = before.data.some(
        (value, index) => Math.abs(value - after.data[index]) > 2,
      );
      if (changed && process.env.TEST_UNDECLARED_OUTPUTS_DIR) {
        await writeFile(
          join(process.env.TEST_UNDECLARED_OUTPUTS_DIR, "animated.png"),
          animated,
        );
        await writeFile(
          join(process.env.TEST_UNDECLARED_OUTPUTS_DIR, "final.png"),
          final,
        );
      }
      assert.equal(
        changed,
        false,
        "heading appearance changes at handwriting completion",
      );
    },
  );
}

async function tabStops(page, steps) {
  const stops = [];
  for (let index = 0; index < steps; index++) {
    await page.keyboard.press("Tab");
    stops.push(
      await page.evaluate(() => ({
        href: document.activeElement?.getAttribute("href"),
        inHero: Boolean(document.activeElement?.closest("home-hero-animation")),
        inDetail: Boolean(
          document.activeElement?.closest("[data-animation-detail]"),
        ),
      })),
    );
  }
  return stops;
}

await withPage(
  {
    route: "/tests/hero/",
    clock: true,
    initialPaintSelector: ".hero-actions",
    recordAnimationEvents: true,
  },
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
      pendingStops.every((stop) => !stop.inDetail),
      "hidden hero details are untabbable throughout keyboard traversal",
    );
    await page.clock.runFor(1484);
    for (const selector of details) {
      const state = await presentation(page, selector);
      assert.ok(
        state.opacity === 0 || state.visibility === "hidden",
        `${selector} stays hidden while heading writes`,
      );
    }
    assert.ok((await page.locator("h1 .handwriting-overlay").count()) > 0);
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
    const records = await page.evaluate(() => window.__animationRecords);
    assert.equal(records.length, 1);
    assert.equal(records[0].level, "debug");
    assert.equal(records[0].event.outcome, "completed");
    assert.equal(records[0].event.phase, "reveal");
    assert.equal(records[0].event.section, "hero");
    assert.ok(records[0].event.elapsedMs > 0);
    assert.deepEqual(Object.keys(records[0].event).sort(), [
      "elapsedMs",
      "outcome",
      "phase",
      "runId",
      "section",
      "type",
    ]);
    await page.setViewportSize({ width: 1100, height: 900 });
    assert.equal(
      await page.evaluate(() => window.__animationRecords.length),
      1,
      "resize after completion cannot replace or repeat terminal outcome",
    );
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
    if (
      options.javaScriptEnabled === false ||
      options.reducedMotion === "reduce"
    ) {
      assert.equal(
        await page.locator(".hero-replay").isDisabled(),
        true,
        "unavailable replay is disabled",
      );
    }
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
    {
      const art = await page.locator(".hero-art").evaluate((element) => ({
        opacity: getComputedStyle(element).opacity,
        translate: getComputedStyle(element).translate,
      }));
      assert.equal(
        art.opacity,
        "1",
        "static fallback does not animate the illustration",
      );
      assert.equal(art.translate, "none");
    }
  });
}

await withPage(
  { route: "/tests/hero/", holdAnimationModules: true, clock: true },
  async (page, { releaseModules }) => {
    await page.locator("h1").waitFor({ state: "attached" });
    await page.locator("h1").evaluate((heading) => {
      heading.firstChild.textContent = "Unsupported XYZ ";
    });
    await page.evaluate(() => document.fonts.ready);
    releaseModules();
    await page.evaluate(() =>
      customElements.whenDefined("home-hero-animation"),
    );
    await page.clock.runFor(200);
    assert.equal(await page.locator("h1 .handwriting-overlay").count(), 0);
    assert.equal(
      (await presentation(page, ".hero-actions")).visibility,
      "visible",
    );
    const art = await presentation(page, ".hero-art");
    assert.equal(
      art.opacity,
      1,
      "failed handwriting initialization keeps artwork static",
    );
  },
);

await withPage(
  {
    route: "/tests/hero/",
    blockAnimationModule: true,
    recordAnimationEvents: true,
  },
  async (page) => {
    await page.waitForTimeout(2200);
    assert.equal(
      await page.locator(".hero-replay").isDisabled(),
      true,
      "blocked module cannot advertise an available replay",
    );
    const records = await page.evaluate(() => window.__animationRecords);
    assert.equal(records.length, 1);
    assert.equal(records[0].level, "warn");
    assert.equal(records[0].event.reason, "watchdog_expired");
    assert.equal(
      (await presentation(page, ".hero-actions")).visibility,
      "visible",
      "module failure restores static controls within two seconds",
    );
  },
);

for (const [reason, change] of [
  [
    "preference",
    async (page) => page.emulateMedia({ reducedMotion: "reduce" }),
  ],
  ["resize", async (page) => page.setViewportSize({ width: 900, height: 800 })],
]) {
  await withPage(
    { route: "/tests/hero/", recordAnimationEvents: true },
    async (page) => {
      await page
        .locator("home-hero-animation[data-animation-pending]")
        .waitFor();
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
        await page
          .locator("home-hero-animation[data-animation-pending]")
          .count(),
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
      const records = await page.evaluate(() => window.__animationRecords);
      assert.equal(records.length, 1);
      assert.equal(records[0].level, "debug");
      assert.equal(records[0].event.outcome, "cancelled");
      assert.equal(records[0].event.reason, reason);
    },
  );
}

await withPage(
  { route: "/tests/hero/", fontMode: "delayed", recordAnimationEvents: true },
  async (page) => {
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
    const records = await page.evaluate(() => window.__animationRecords);
    assert.equal(records.length, 1);
    assert.equal(records[0].event.reason, "disconnection");
    assert.equal(records[0].level, "debug");
  },
);

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

await withPage(
  { route: "/tests/lifecycle/", recordAnimationEvents: true },
  async (page) => {
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
    const records = await page.evaluate(() => window.__animationRecords);
    assert.equal(records.length, 4);
    assert.equal(new Set(records.map(({ event }) => event.runId)).size, 4);
    assert.deepEqual(
      records.map(({ event }) => event.reason ?? event.outcome).sort(),
      ["completed", "completed", "exception", "initialization_unavailable"],
    );
    for (const { event, level } of records) {
      assert.equal(level, event.outcome === "fallback" ? "warn" : "debug");
      assert.deepEqual(
        Object.keys(event).sort(),
        event.outcome === "completed"
          ? ["elapsedMs", "outcome", "phase", "runId", "section", "type"]
          : [
              "elapsedMs",
              "outcome",
              "phase",
              "reason",
              "runId",
              "section",
              "type",
            ],
      );
    }
    assert.equal(
      (await presentation(page, ".lifecycle-fixture:nth-of-type(4) a"))
        .visibility,
      "visible",
    );
  },
);

// Inspect structured records at the console boundary, never rendered messages.
await withPage(
  { route: "/tests/hero/", fontMode: "blocked", recordAnimationEvents: true },
  async (page) => {
    await page.waitForFunction(() => window.__animationRecords?.length > 0);
    const records = await page.evaluate(() => window.__animationRecords);
    assert.equal(records.length, 1);
    assert.equal(records[0].level, "warn");
    assert.equal(records[0].event.type, "SectionAnimationSettled");
    assert.equal(records[0].event.outcome, "fallback");
    assert.equal(records[0].event.reason, "font_unavailable");
    assert.equal(
      (await presentation(page, ".hero-actions")).visibility,
      "visible",
    );
    assert.equal(
      await page
        .locator(".hero-actions a")
        .first()
        .evaluate((link) => {
          link.focus();
          return document.activeElement === link;
        }),
      true,
    );
  },
);

for (const held of ["holdAnimationModules", "fontMode"]) {
  await withPage(
    {
      route: "/tests/hero/",
      clock: true,
      recordAnimationEvents: true,
      ...(held === "fontMode"
        ? { fontMode: "held" }
        : { holdAnimationModules: true }),
    },
    async (page, { releaseFonts, releaseModules }) => {
      await page.locator("h1").waitFor({ state: "attached" });
      await page.clock.runFor(2200);
      assert.equal(
        (await presentation(page, ".hero-actions")).visibility,
        "visible",
      );
      assert.equal(
        await page
          .locator(".hero-actions a")
          .first()
          .evaluate((link) => {
            link.focus();
            return document.activeElement === link;
          }),
        true,
      );
      const records = await page.evaluate(() => window.__animationRecords);
      assert.equal(records.length, 1);
      assert.equal(records[0].event.reason, "watchdog_expired");
      releaseFonts();
      releaseModules();
      await page.evaluate(() => document.fonts.ready);
      await page.evaluate(() =>
        customElements.whenDefined("home-hero-animation"),
      );
      await page.clock.runFor(6000);
      assert.equal(
        await page.evaluate(() => window.__animationRecords.length),
        1,
        "late initialization must not repeat watchdog outcome",
      );
      assert.equal(
        (await presentation(page, ".hero-actions")).visibility,
        "visible",
      );
    },
  );
}

await withPage(
  {
    route: "/tests/hero/",
    clock: true,
    recordAnimationEvents: true,
    throwAnimationListener: true,
  },
  async (page) => {
    await page.evaluate(() => document.fonts.ready);
    await page.evaluate(() =>
      customElements.whenDefined("home-hero-animation"),
    );
    await page.clock.runFor(1);
    await page
      .locator("h1 .handwriting-overlay")
      .first()
      .waitFor({ state: "attached" });
    await page.clock.runFor(6000);
    assert.equal(
      (await presentation(page, ".hero-actions")).visibility,
      "visible",
    );
    assert.equal(
      await page
        .locator(".hero-actions a")
        .first()
        .evaluate((link) => {
          link.focus();
          return document.activeElement === link;
        }),
      true,
    );
    const records = await page.evaluate(() => window.__animationRecords);
    assert.equal(
      records.length,
      1,
      "failing listener neither changes completion nor recursively emits",
    );
    assert.equal(records[0].event.outcome, "completed");
  },
);

// Replay is a real button interaction and must leave the hero copy untouched.
await withPage(
  { route: "/tests/hero/", clock: true, hasTouch: true },
  async (page) => {
    await page.evaluate(() => document.fonts.ready);
    await page.evaluate(() =>
      customElements.whenDefined("home-hero-animation"),
    );
    await page.clock.runFor(6000);
    const button = page.getByRole("button", {
      name: "Replay computer and plant animation",
    });
    assert.equal(
      await button.count(),
      1,
      "illustration must be keyboard accessible",
    );
    const art = button.locator("svg");
    const rest = await art.evaluate((el) => getComputedStyle(el).opacity);
    const plant = page.locator(".hero-plant");
    const flourish = page.locator(".hero-flourish");
    const restingPlant = await plant.evaluate(
      (el) => getComputedStyle(el).transform,
    );
    const restingFlourish = await flourish.evaluate(
      (el) => getComputedStyle(el).strokeDasharray,
    );
    for (const activation of ["click", "Enter", "Space", "tap", "click"]) {
      if (activation === "click") await button.click();
      else if (activation === "tap") await button.tap();
      else await button.press(activation);
      await page.clock.runFor(250);
      assert.ok(
        Number(await art.evaluate((el) => getComputedStyle(el).opacity)) <
          Number(rest),
      );
      assert.equal(await page.locator("h1 .handwriting-overlay").count(), 0);
      assert.equal(
        (await presentation(page, ".hero-actions")).visibility,
        "visible",
      );
    }
    await page.clock.runFor(550);
    assert.notEqual(
      await plant.evaluate((el) => getComputedStyle(el).transform),
      restingPlant,
      "plant sways during replay",
    );
    assert.notEqual(
      await flourish.evaluate((el) => getComputedStyle(el).strokeDasharray),
      restingFlourish,
      "accent strokes redraw during replay",
    );
    await page.clock.runFor(2000);
    assert.equal(
      await plant.evaluate((el) => getComputedStyle(el).transform),
      restingPlant,
    );
    assert.equal(
      await flourish.evaluate((el) => getComputedStyle(el).strokeDasharray),
      restingFlourish,
    );
    assert.equal(
      await art.evaluate((el) => getComputedStyle(el).opacity),
      rest,
    );
    await button.click();
    await page.clock.runFor(800);
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.clock.runFor(16);
    assert.equal(
      await plant.evaluate((el) => getComputedStyle(el).transform),
      restingPlant,
      "motion preference cancels an active replay",
    );
    assert.equal(await button.isDisabled(), true);
    await button.evaluate((element) => element.click());
    await page.clock.runFor(250);
    assert.equal(
      await art.evaluate((el) => getComputedStyle(el).opacity),
      rest,
    );
    await page.emulateMedia({ reducedMotion: "no-preference" });
    await page.clock.runFor(16);
    // Media-query changes arrive asynchronously; activation waits for availability.
    await button.click();
    assert.equal(await button.isEnabled(), true);
    await page.clock.runFor(250);
    assert.ok(
      Number(await art.evaluate((el) => getComputedStyle(el).opacity)) <
        Number(rest),
    );
    assert.equal(
      await button.evaluate((element) => {
        const root = element.closest("home-hero-animation");
        root.remove();
        return element.disabled;
      }),
      true,
      "disconnection disables replay until remounted",
    );
  },
);
