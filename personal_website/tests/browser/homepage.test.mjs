import assert from "node:assert/strict";
import { presentation, withPage } from "./harness.mjs";

const sections = [
  {
    root: "home-hero-animation",
    heading: "#intro-heading",
    detail: ".hero-actions",
  },
  {
    root: "featured-work-animation",
    heading: "#featured-heading",
    detail: ".entry-link",
  },
  {
    root: "recent-writing-animation",
    heading: "#recent-heading",
    detail: ".entry-link",
  },
];

async function visible(page, section) {
  return page
    .locator(`${section.root} ${section.detail}`)
    .first()
    .evaluate((element) => {
      const style = getComputedStyle(element);
      return style.visibility === "visible" && Number(style.opacity) > 0.99;
    });
}

async function writing(page, section) {
  assert.ok(
    await page
      .locator(`${section.root} ${section.heading} .handwriting-word`)
      .count(),
    `${section.root} is actively writing`,
  );
  assert.equal(await visible(page, section), false);
}

async function readable(page) {
  for (const section of sections) {
    assert.equal(
      await visible(page, section),
      true,
      `${section.root} is readable`,
    );
    assert.equal(
      await page
        .locator(`${section.root} ${section.detail}`)
        .first()
        .evaluate((element) => element.inert),
      false,
      `${section.root} detail is focusable`,
    );
  }
  await page.locator("home-hero-animation .paper-button").focus();
  assert.equal(
    await page
      .locator("home-hero-animation .paper-button")
      .evaluate((element) => document.activeElement === element),
    true,
    "hero action accepts keyboard focus",
  );
}

async function noHorizontalOverflow(page, label) {
  const widths = await page.evaluate(() => ({
    document: document.documentElement.scrollWidth,
    viewport: document.documentElement.clientWidth,
  }));
  assert.ok(
    widths.document <= widths.viewport,
    `${label}: no horizontal overflow: ${JSON.stringify(widths)}`,
  );
}

// A production page, with all three real components, proves the visible
// section never waits for an offscreen sibling to finish.
await withPage({ route: "/", clock: true }, async (page) => {
  assert.equal(await page.locator("home-hero-animation").count(), 1);
  assert.equal(await page.locator("featured-work-animation").count(), 1);
  assert.equal(await page.locator("recent-writing-animation").count(), 1);
  await page.evaluate(() => document.fonts.ready);
  await page.clock.runFor(350);
  await writing(page, sections[0]);
  assert.equal(await visible(page, sections[1]), false);
  assert.equal(await visible(page, sections[2]), false);
  await page.locator("#featured-heading").scrollIntoViewIfNeeded();
  await page.waitForTimeout(150);
  await page.clock.runFor(200);
  await writing(page, sections[1]);
  await page.locator("#recent-heading").scrollIntoViewIfNeeded();
  await page.waitForTimeout(150);
  await page.clock.runFor(300);
  await writing(page, sections[2]);
  assert.equal(
    await visible(page, sections[0]),
    false,
    "hero still owns its details",
  );
  await page.clock.runFor(2200);
  assert.equal(
    await visible(page, sections[2]),
    true,
    "recent writing completes independently",
  );
  assert.equal(
    await visible(page, sections[1]),
    true,
    "featured work finished its own writing",
  );
  await page.clock.runFor(1500);
  assert.equal(await visible(page, sections[0]), true);
  await page.locator(".site-nav a[href='/blog/']").click();
  await page.waitForURL("**/blog/");
  assert.equal((await page.locator(".entry-link").count()) > 0, true);
  assert.equal(
    await page.locator("[data-animation-pending]").count(),
    0,
    "blog listing remains static",
  );
  assert.equal(
    await page.locator(".handwriting-word").count(),
    0,
    "blog heading never writes",
  );
  assert.equal(await page.locator(".entry-link").first().isVisible(), true);
});
console.log("homepage_sections_run_independently passed");

// Navigation must dispose the old instance while handwriting is still active.
await withPage({ route: "/" }, async (page) => {
  await page.evaluate(() => document.fonts.ready);
  for (let visit = 1; visit <= 2; visit++) {
    await page
      .locator("home-hero-animation #intro-heading .handwriting-word")
      .first()
      .waitFor();
    await writing(page, sections[0]);
    const oldRoot = await page.locator("home-hero-animation").elementHandle();
    await page.locator(".site-nav a[href='/blog/']").click();
    await page.waitForURL("**/blog/");
    assert.equal(
      await oldRoot.evaluate((root) => root.isConnected),
      false,
      `visit ${visit} disposed`,
    );
    assert.equal(
      await page.locator(".entry-link[data-animation-detail]").count(),
      0,
    );
    await page.locator(".site-brand").click();
    await page.waitForURL((url) => url.pathname === "/");
    await page
      .locator("home-hero-animation #intro-heading .handwriting-word")
      .first()
      .waitFor();
    await writing(page, sections[0]);
  }
  await page
    .locator("home-hero-animation .hero-actions")
    .waitFor({ state: "visible" });
  await page.waitForFunction(
    () =>
      getComputedStyle(document.querySelector(".hero-actions")).opacity > 0.99,
  );
  assert.equal(
    await page.locator("home-hero-animation .hero-actions").count(),
    1,
    "one reveal on return",
  );
});
console.log(
  "navigation_disposes_pending_work: two active-writing visits passed",
);

await withPage({ route: "/", fontMode: "delayed" }, async (page) => {
  const oldRoot = await page.locator("home-hero-animation").elementHandle();
  assert.equal(
    await oldRoot.evaluate((root) =>
      root.hasAttribute("data-animation-pending"),
    ),
    true,
  );
  await page.locator(".site-nav a[href='/blog/']").click();
  await page.waitForURL("**/blog/");
  await page.locator(".site-brand").click();
  await page.waitForURL((url) => url.pathname === "/");
  await page.waitForTimeout(2800);
  assert.equal(await oldRoot.evaluate((root) => root.isConnected), false);
  assert.equal(
    await oldRoot.evaluate((root) =>
      root.hasAttribute("data-animation-pending"),
    ),
    false,
  );
  assert.equal(await page.locator("home-hero-animation").count(), 1);
  await page.waitForFunction(
    () => {
      const style = getComputedStyle(
        document.querySelector("home-hero-animation .hero-actions"),
      );
      return style.visibility === "visible" && Number(style.opacity) > 0.99;
    },
    null,
    { timeout: 10000 },
  );
  assert.equal(
    await visible(page, sections[0]),
    true,
    "new visit stays readable after late font resolution",
  );
});
console.log("navigation_disposes_pending_work: delayed font passed");

for (const options of [
  { blockAnimationModule: true },
  { javaScriptEnabled: false },
  { reducedMotion: "reduce" },
  { fontMode: "blocked" },
]) {
  await withPage(
    {
      route: "/",
      trackPending: Boolean(options.blockAnimationModule),
      ...options,
    },
    async (page) => {
      if (options.blockAnimationModule) {
        const startedAt = await page.evaluate(() => window.__pendingStartedAt);
        assert.equal(
          typeof startedAt,
          "number",
          "bootstrap activation was observed",
        );
        await page.waitForFunction(
          () =>
            [
              ...document.querySelectorAll(
                "home-hero-animation .hero-actions, featured-work-animation .entry-link, recent-writing-animation .entry-link",
              ),
            ].every(
              (element) => getComputedStyle(element).visibility === "visible",
            ),
          null,
          { timeout: 2300 },
        );
        const elapsed = await page.evaluate(
          (start) => performance.now() - start,
          startedAt,
        );
        assert.ok(
          elapsed <= 2150,
          `blocked module restored static details within 2s plus 150ms scheduling tolerance: ${elapsed}ms`,
        );
      } else await page.waitForTimeout(100);
      await readable(page);
    },
  );
  console.log(
    `fallback_preserves_all_content: ${JSON.stringify(options)} passed`,
  );
}

for (const theme of ["light", "dracula"]) {
  for (const width of [390, 430, 1440]) {
    await withPage(
      {
        route: "/",
        viewport: { width, height: width === 1440 ? 900 : 844 },
        clock: true,
      },
      async (page) => {
        await page.evaluate((value) => {
          document.documentElement.dataset.theme = value;
        }, theme);
        await page.evaluate(() => document.fonts.ready);
        await page.clock.runFor(250);
        await writing(page, sections[0]);
        await noHorizontalOverflow(page, `${theme} ${width}px before reveal`);
        const before = await presentation(page, "featured-work-animation");
        await page.clock.runFor(5000);
        const after = await presentation(page, "featured-work-animation");
        assert.ok(
          Math.abs(after.box.y - before.box.y) < 1,
          `${theme} ${width}px: featured slot stays reserved`,
        );
        await noHorizontalOverflow(page, `${theme} ${width}px after reveal`);
        if (width !== 1440) {
          await page.locator("#featured-heading").scrollIntoViewIfNeeded();
          await page.waitForTimeout(150);
          await page.clock.runFor(200);
          await writing(page, sections[1]);
          const recentBefore = await presentation(
            page,
            "recent-writing-animation",
          );
          await page.setViewportSize({ width: 844, height: width });
          await page.waitForFunction(
            () =>
              !document
                .querySelector("featured-work-animation")
                ?.hasAttribute("data-animation-pending"),
            null,
            { timeout: 1000 },
          );
          assert.equal(
            await visible(page, sections[1]),
            true,
            "orientation change restores featured content",
          );
          assert.equal(
            await page
              .locator("featured-work-animation .handwriting-word")
              .count(),
            0,
          );
          await noHorizontalOverflow(page, `${theme} ${width}px landscape`);
          await page.setViewportSize({ width, height: 844 });
          const recentAfter = await presentation(
            page,
            "recent-writing-animation",
          );
          assert.ok(
            Math.abs(recentAfter.box.y - recentBefore.box.y) < 1,
            "recent slot retains its position after orientation returns",
          );
          await noHorizontalOverflow(page, `${theme} ${width}px restored`);
        }
      },
    );
    console.log(`mobile_preserves_layout: ${theme} ${width}px passed`);
  }
}
