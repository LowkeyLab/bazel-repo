import assert from "node:assert/strict";
import { presentation, withPage } from "./harness.mjs";

async function entryStates(
  page,
  root = "featured-work-animation:first-of-type",
) {
  return page.locator(`${root} .entry-link`).evaluateAll((entries) =>
    entries.map((entry) => {
      const style = getComputedStyle(entry);
      return {
        opacity: Number(style.opacity),
        visibility: style.visibility,
        inert: entry.inert,
      };
    }),
  );
}

await withPage(
  { route: "/tests/featured-work/", clock: true },
  async (page) => {
    await page.evaluate(() => document.fonts.ready);
    const entries = page.locator(
      "featured-work-animation:first-of-type .entry-link",
    );
    assert.equal(await page.locator("#featured-heading").count(), 1);
    assert.equal(
      await page
        .locator("featured-work-animation:first-of-type .home-section")
        .getAttribute("aria-labelledby"),
      "featured-heading",
    );
    assert.equal(
      await entries.count(),
      3,
      "fixture renders three representative entries",
    );
    for (let index = 0; index < 3; index++) {
      const entry = entries.nth(index);
      assert.equal(await entry.locator(".entry-title").count(), 1);
      for (const selector of [
        "time",
        ".entry-description",
        ".entry-meta",
        ".entry-type",
        ".tags",
      ]) {
        assert.equal(
          await entry.locator(selector).count(),
          0,
          `title-only entry ${index} omits ${selector}`,
        );
      }
    }
    const laterStart = await presentation(page, "#later-section");
    await page.clock.runFor(500);
    const pending = await entryStates(page);
    assert.ok(
      pending.every(
        (entry) => entry.visibility === "hidden" || entry.opacity === 0,
      ),
      "all complete entries stay hidden while the heading writes",
    );
    assert.ok(
      pending.every((entry) => entry.inert),
      "hidden entries are not keyboard reachable",
    );
    for (const selector of [".entry-title"]) {
      assert.equal(
        (
          await presentation(
            page,
            `featured-work-animation:first-of-type li:first-child .entry-link ${selector}`,
          )
        ).visibility,
        "hidden",
        `${selector} stays hidden with its complete entry`,
      );
    }
    await page.clock.runFor(780);
    const stagger = await entryStates(page);
    assert.ok(
      stagger[0].opacity > stagger[1].opacity &&
        stagger[1].opacity >= stagger[2].opacity,
      "complete entries reveal in DOM order after writing",
    );
    await page.clock.runFor(900);
    const revealed = await entryStates(page);
    assert.ok(
      revealed.every(
        (entry) =>
          entry.visibility === "visible" &&
          entry.opacity > 0.99 &&
          !entry.inert,
      ),
      "every complete entry finishes visible and focusable",
    );
    for (const selector of [".entry-title"]) {
      assert.equal(
        (
          await presentation(
            page,
            `featured-work-animation:first-of-type li:first-child .entry-link ${selector}`,
          )
        ).visibility,
        "visible",
        `${selector} reveals with its complete entry`,
      );
    }
    const laterEnd = await presentation(page, "#later-section");
    assert.ok(
      Math.abs(laterEnd.box.y - laterStart.box.y) < 1,
      "later sections retain their position",
    );

    await page.locator("#long-featured-heading").scrollIntoViewIfNeeded();
    await page.waitForTimeout(150);
    await page.clock.runFor(1800);
    const longPending = await entryStates(
      page,
      "featured-work-animation:nth-of-type(2)",
    );
    assert.equal(
      longPending[0].visibility,
      "hidden",
      "longer handwriting keeps its entry hidden beyond the default duration",
    );
    await page.clock.runFor(1500);
    const longRevealed = await entryStates(
      page,
      "featured-work-animation:nth-of-type(2)",
    );
    assert.ok(
      longRevealed[0].opacity > 0.99 &&
        longRevealed[0].visibility === "visible",
      "entry reveal follows the changed handwriting completion",
    );
  },
);

await withPage(
  { route: "/tests/featured-work/", clock: true },
  async (page) => {
    await page.evaluate(() => document.fonts.ready);
    await page.clock.runFor(16);
    await page.locator("#empty-featured-heading").scrollIntoViewIfNeeded();
    await page.waitForTimeout(150);
    const emptyPending = await presentation(
      page,
      "#empty-featured-heading + .entry-list p",
    );
    assert.equal(
      emptyPending.visibility,
      "hidden",
      "empty-list message follows its heading",
    );
    await page.clock.runFor(2000);
    const emptyRevealed = await presentation(
      page,
      "#empty-featured-heading + .entry-list p",
    );
    assert.equal(
      emptyRevealed.visibility,
      "visible",
      "empty-list message reveals after writing",
    );
  },
);

await withPage({ route: "/tests/independence/", clock: true }, async (page) => {
  await page.evaluate(() => document.fonts.ready);
  const roots = page.locator("featured-work-animation");
  assert.equal(await roots.count(), 2);
  assert.equal(await page.locator("#first-featured-heading").count(), 1);
  assert.equal(await page.locator("#second-featured-heading").count(), 1);
  await page.clock.runFor(500);
  assert.ok(
    (await roots.first().locator(".handwriting-overlay").count()) > 0,
    "first heading has temporary handwriting markup when its root is removed",
  );
  await roots.first().evaluate((root) => root.remove());
  const siblingBefore = await entryStates(page, "featured-work-animation");
  assert.ok(
    siblingBefore.every(
      (entry) => entry.visibility === "hidden" || entry.opacity === 0,
    ),
    "offscreen sibling keeps its own pending state after the first disconnects",
  );
  await page.locator("#second-featured-heading").scrollIntoViewIfNeeded();
  await page.waitForTimeout(150);
  await page.clock.runFor(200);
  assert.ok(
    (await roots.locator(".handwriting-overlay").count()) > 0,
    "sibling starts its own handwriting after entering the viewport",
  );
  const siblingWriting = await entryStates(page, "featured-work-animation");
  assert.ok(
    siblingWriting.every((entry) => entry.visibility === "hidden"),
    "sibling entries remain hidden during their own heading animation",
  );
  await page.clock.runFor(2200);
  const siblingAfter = await entryStates(page, "featured-work-animation");
  assert.ok(
    siblingAfter.every(
      (entry) => entry.visibility === "visible" && entry.opacity > 0.99,
    ),
    "sibling writes and reveals after scrolling into view",
  );
});

for (const theme of ["light", "dracula"]) {
  await withPage(
    {
      route: "/tests/featured-work/",
      viewport: { width: 390, height: 844 },
      reducedMotion: "reduce",
    },
    async (page) => {
      await page.evaluate((value) => {
        document.documentElement.dataset.theme = value;
      }, theme);
      const size = await page.evaluate(() => ({
        scroll: document.documentElement.scrollWidth,
        client: document.documentElement.clientWidth,
      }));
      assert.ok(
        size.scroll <= size.client,
        `${theme}: long titles do not overflow at 390px`,
      );
      const empty = await presentation(
        page,
        "#empty-featured-heading + .entry-list p",
      );
      assert.equal(
        empty.visibility,
        "visible",
        `${theme}: empty list text is visible`,
      );
    },
  );
}

await withPage({ route: "/tests/lifecycle/" }, async (page) => {
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(100);
  const unsupported = await presentation(page, "a[href='#unsupported']");
  assert.equal(
    unsupported.visibility,
    "visible",
    "unsupported heading restores readable detail",
  );
});
