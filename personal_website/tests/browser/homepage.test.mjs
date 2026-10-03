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
      .locator(`${section.root} ${section.heading} .handwriting-overlay`)
      .count(),
    `${section.root} is actively writing`,
  );
  assert.equal(await visible(page, section), false);
}

async function readable(page) {
  for (const [root, selector] of [
    ["home-hero-animation", "[data-animation-detail]"],
    ["featured-work-animation", ".entry-link"],
    ["recent-writing-animation", ".entry-link, .all-writing"],
  ]) {
    const details = await page
      .locator(`${root} ${selector}`)
      .evaluateAll((elements) =>
        elements.map((element) => {
          const style = getComputedStyle(element);
          return {
            text: element.textContent.trim(),
            visibility: style.visibility,
            opacity: Number(style.opacity),
            inert: element.inert,
          };
        }),
      );
    assert.ok(details.length > 0, `${root} has rendered details`);
    for (const detail of details) {
      assert.ok(detail.text, `${root} detail has readable text`);
      assert.equal(
        detail.visibility,
        "visible",
        `${root}: ${detail.text} is visible`,
      );
      assert.ok(detail.opacity > 0.99, `${root}: ${detail.text} is opaque`);
      assert.equal(detail.inert, false, `${root}: ${detail.text} is not inert`);
    }
  }
  const entryParts = await page
    .locator(
      "featured-work-animation .entry-link .entry-title, featured-work-animation .entry-link time, featured-work-animation .entry-link .entry-description, featured-work-animation .entry-link .entry-meta, featured-work-animation .entry-link .entry-type, featured-work-animation .entry-link .tags, featured-work-animation .entry-link .paper-tag, recent-writing-animation .entry-link .entry-title, recent-writing-animation .entry-link time, recent-writing-animation .entry-link .entry-description, recent-writing-animation .entry-link .entry-meta, recent-writing-animation .entry-link .entry-type, recent-writing-animation .entry-link .tags, recent-writing-animation .entry-link .paper-tag",
    )
    .evaluateAll((elements) =>
      elements.map((element) => ({
        text: element.textContent.trim(),
        visibility: getComputedStyle(element).visibility,
        opacity: Number(getComputedStyle(element).opacity),
      })),
    );
  assert.ok(entryParts.length > 0, "article metadata is rendered");
  for (const part of entryParts) {
    assert.ok(part.text, "article detail has readable text");
    assert.equal(part.visibility, "visible", `${part.text} is visible`);
    assert.ok(part.opacity > 0.99, `${part.text} is opaque`);
  }
  const controls = await page
    .locator(
      "home-hero-animation a[href], featured-work-animation a[href], recent-writing-animation a[href]",
    )
    .evaluateAll((links) =>
      links.map((link) => {
        link.focus({ preventScroll: true });
        return {
          href: link.getAttribute("href"),
          focusable: document.activeElement === link,
          inert: link.inert || Boolean(link.closest("[inert]")),
          visibility: getComputedStyle(link).visibility,
        };
      }),
    );
  assert.ok(controls.length > 0, "all section controls are rendered");
  for (const control of controls) {
    assert.equal(control.visibility, "visible", `${control.href} is visible`);
    assert.equal(control.inert, false, `${control.href} is not inert`);
    assert.equal(control.focusable, true, `${control.href} accepts focus`);
  }
}

async function observeHeroReveals(page) {
  await page.evaluate(() => {
    if (window.__heroObservation) window.__heroObservation.stopped = true;
    const observation = {
      root: null,
      roots: 0,
      writingStarts: 0,
      reveals: 0,
      regressions: 0,
      writing: false,
      revealed: false,
      stopped: false,
    };
    window.__heroObservation = observation;
    const sample = () => {
      if (observation.stopped) return;
      const root = document.querySelector("home-hero-animation");
      if (root !== observation.root) {
        observation.root = root;
        observation.writing = false;
        observation.revealed = false;
        if (root) observation.roots++;
      }
      if (root) {
        const writing = Boolean(
          root.querySelector("#intro-heading .handwriting-overlay"),
        );
        const detail = root.querySelector(".hero-actions");
        const style = getComputedStyle(detail);
        const revealed =
          style.visibility === "visible" && Number(style.opacity) > 0.99;
        if (writing && !observation.writing) observation.writingStarts++;
        if (revealed && !observation.revealed) observation.reveals++;
        if (!revealed && observation.revealed) observation.regressions++;
        observation.writing = writing;
        observation.revealed = revealed;
      }
      requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  });
}

async function observedHero(page) {
  return page.evaluate(() => {
    const { roots, writingStarts, reveals, regressions } =
      window.__heroObservation;
    return { roots, writingStarts, reveals, regressions };
  });
}

async function heroCompleted(page) {
  await page.waitForFunction(
    () => {
      const detail = document.querySelector(
        "home-hero-animation .hero-actions",
      );
      if (!detail) return false;
      const style = getComputedStyle(detail);
      return style.visibility === "visible" && Number(style.opacity) > 0.99;
    },
    null,
    { timeout: 10000 },
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

async function staticHeroArtwork(page) {
  const artwork = await page
    .locator(".hero-art, .hero-flourish, .hero-plant, .hero-code")
    .evaluateAll((elements) =>
      elements.map((element) => {
        const style = getComputedStyle(element);
        return {
          opacity: style.opacity,
          translate: style.translate,
          rotate: style.rotate,
          dasharray: style.strokeDasharray,
        };
      }),
    );
  assert.equal(artwork.length, 4);
  for (const state of artwork) {
    assert.deepEqual(
      state,
      { opacity: "1", translate: "none", rotate: "none", dasharray: "none" },
      "fallback artwork stays fully drawn and stationary",
    );
  }
}

// Hold the real font response beyond the watchdog, including a controller
// that arrives only after the bootstrap has already restored static content.
const expiryFailures = [];
for (const holdAnimationModules of [false, true]) {
  try {
    await withPage(
      { route: "/", fontMode: "held", holdAnimationModules },
      async (page, { releaseFonts, releaseModules }) => {
        await page
          .locator("recent-writing-animation")
          .waitFor({ state: "attached" });
        await page.waitForFunction(() => document.fonts.status === "loading");
        await page.waitForTimeout(2200);
        assert.equal(
          await page.evaluate(() => document.fonts.status),
          "loading",
        );
        await readable(page);
        await staticHeroArtwork(page);
        if (holdAnimationModules) {
          releaseModules();
          await page.evaluate(() =>
            Promise.all([
              customElements.whenDefined("home-hero-animation"),
              customElements.whenDefined("featured-work-animation"),
              customElements.whenDefined("recent-writing-animation"),
            ]),
          );
          await page.waitForTimeout(100);
          assert.equal(
            await page.evaluate(() => document.fonts.status),
            "loading",
          );
          await readable(page);
          await staticHeroArtwork(page);
        }
        releaseFonts();
        await page.evaluate(() => document.fonts.ready);
        await page.waitForTimeout(250);
        await readable(page);
        await staticHeroArtwork(page);
        assert.equal(
          await page.locator(".handwriting-overlay").count(),
          0,
          "expired initialization cannot restart after fonts arrive",
        );
      },
    );
    console.log(
      `watchdog_restores_focus: late module=${holdAnimationModules} passed`,
    );
  } catch (error) {
    expiryFailures.push(
      `late module=${holdAnimationModules}: ${error.message}`,
    );
  }
}
assert.deepEqual(expiryFailures, [], "font watchdog and late module fallback");

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
  await page
    .locator("featured-work-animation .handwriting-overlay")
    .first()
    .waitFor();
  await page.clock.runFor(200);
  await writing(page, sections[1]);
  await page.locator("#recent-heading").scrollIntoViewIfNeeded();
  await page
    .locator("recent-writing-animation .handwriting-overlay")
    .first()
    .waitFor();
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
    await page.locator(".handwriting-overlay").count(),
    0,
    "blog heading never writes",
  );
  assert.equal(await page.locator(".entry-link").first().isVisible(), true);
});
console.log("homepage_sections_run_independently passed");

// Navigation must dispose the old instance while handwriting is still active.
await withPage({ route: "/" }, async (page) => {
  await page.evaluate(() => document.fonts.ready);
  await page
    .locator("home-hero-animation #intro-heading .handwriting-overlay")
    .first()
    .waitFor();
  await writing(page, sections[0]);
  for (let visit = 1; visit <= 2; visit++) {
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
    await observeHeroReveals(page);
    await page.locator(".site-brand").click();
    await page.waitForURL((url) => url.pathname === "/");
    await page
      .locator("home-hero-animation #intro-heading .handwriting-overlay")
      .first()
      .waitFor();
    await writing(page, sections[0]);
    await heroCompleted(page);
    await page.waitForFunction(
      () => window.__heroObservation?.reveals === 1,
      null,
      {
        timeout: 1000,
      },
    );
    await page.waitForTimeout(350);
    assert.deepEqual(
      await observedHero(page),
      { roots: 1, writingStarts: 1, reveals: 1, regressions: 0 },
      `visit ${visit} writes and reveals exactly once`,
    );
  }
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
  await observeHeroReveals(page);
  await page.locator(".site-brand").click();
  await page.waitForURL((url) => url.pathname === "/");
  await page.evaluate(() => document.fonts.ready);
  assert.equal(await oldRoot.evaluate((root) => root.isConnected), false);
  assert.equal(
    await oldRoot.evaluate((root) =>
      root.hasAttribute("data-animation-pending"),
    ),
    false,
  );
  assert.equal(await page.locator("home-hero-animation").count(), 1);
  await heroCompleted(page);
  await page.waitForFunction(
    () => window.__heroObservation?.reveals === 1,
    null,
    {
      timeout: 1000,
    },
  );
  await page.waitForTimeout(350);
  assert.equal(
    await visible(page, sections[0]),
    true,
    "new visit stays readable after late font resolution",
  );
  const observation = await observedHero(page);
  assert.equal(
    observation.roots,
    1,
    "late font resolution keeps one returned hero root",
  );
  assert.ok(
    observation.writingStarts <= 1,
    "late font resolution cannot restart handwriting",
  );
  assert.equal(
    observation.reveals,
    1,
    "late font resolution produces one visible detail reveal",
  );
  assert.equal(observation.regressions, 0, "revealed detail never hides again");
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
          await page
            .locator("featured-work-animation .handwriting-overlay")
            .first()
            .waitFor();
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
              .locator("featured-work-animation .handwriting-overlay")
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

await withPage({ route: "/", reducedMotion: "reduce" }, async (page) => {
  const allWriting = page.getByRole("link", {
    name: "All writing",
    exact: true,
  });
  assert.equal(await allWriting.getAttribute("href"), "/blog/");
  assert.equal(
    await allWriting.evaluate((link) => {
      const list = document.querySelector(
        'section[aria-label="Recent writing"]',
      );
      return (
        !list.contains(link) &&
        Boolean(
          list.compareDocumentPosition(link) & Node.DOCUMENT_POSITION_FOLLOWING,
        )
      );
    }),
    true,
    "All writing follows the recent list",
  );

  // Operate the real accessible checkbox with the keyboard: it is visually
  // hidden, so clicking its zero-sized input would not model a user action.
  const toggle = page.getByRole("checkbox", { name: "Use dark theme" });
  const initialDark = await toggle.isChecked();
  async function assertTheme(dark) {
    await page.waitForFunction((expectedDark) => {
      const checkbox = document.querySelector("#theme-toggle");
      return (
        checkbox?.checked === expectedDark &&
        document.documentElement.dataset.theme ===
          (expectedDark ? "dracula" : "light")
      );
    }, dark);
    assert.equal(await toggle.isChecked(), dark);
    assert.equal(
      await page.locator("html").getAttribute("data-theme"),
      dark ? "dracula" : "light",
    );
  }
  await assertTheme(initialDark);
  for (const dark of [!initialDark, initialDark]) {
    await toggle.focus();
    await page.keyboard.press("Space");
    await assertTheme(dark);
    await page.getByRole("link", { name: "About", exact: true }).click();
    await page.waitForURL("**/about/");
    await assertTheme(dark);
    await page.reload();
    await assertTheme(dark);
    await page.goto(new URL("/", page.url()).href);
    await assertTheme(dark);
  }
});
console.log("all_writing_order_and_theme_persistence: passed");

// The production homepage wires art playback to the surrounding text reveal.
await withPage({ route: "/", clock: true }, async (page) => {
  await page.evaluate(() => document.fonts.ready);
  await page.evaluate(() => customElements.whenDefined("home-hero-animation"));
  let revealing = false;
  for (let frame = 0; frame < 300; frame++) {
    await page.clock.runFor(16);
    if ((await presentation(page, ".eyebrow")).visibility === "visible") {
      revealing = true;
      break;
    }
  }
  assert.ok(revealing, "homepage text reveal starts");
  await page.clock.runFor(250);
  assert.ok((await presentation(page, ".eyebrow")).opacity > 0);
  assert.ok(
    (await presentation(page, ".hero-code")).opacity < 1,
    "homepage illustration animates with the surrounding text",
  );
});
