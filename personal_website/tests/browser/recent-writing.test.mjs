import assert from "node:assert/strict";
import { presentation, withPage } from "./harness.mjs";

const root = "recent-writing-animation";

async function entryStates(page) {
  return page.locator(`${root} .entry-link`).evaluateAll((entries) =>
    entries.map((entry) => ({
      opacity: Number(getComputedStyle(entry).opacity),
      visibility: getComputedStyle(entry).visibility,
      inert: entry.inert,
    })),
  );
}

await withPage(
  { route: "/tests/recent-writing/", clock: true },
  async (page) => {
    const entries = page.locator(`${root} .entry-link`);
    const allWriting = page.locator(`${root} .all-writing`);
    assert.equal(await entries.count(), 3);
    assert.equal(await page.locator("#fixture-recent-heading").count(), 1);
    assert.equal(await allWriting.getAttribute("href"), "/blog/");
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

    await page.evaluate(() => document.fonts.ready);
    await page.locator("#fixture-recent-heading").scrollIntoViewIfNeeded();
    await page.waitForTimeout(150);
    await page.clock.runFor(500);
    const pending = await entryStates(page);
    assert.ok(
      pending.every((entry) => entry.visibility === "hidden" && entry.inert),
      "complete entries stay hidden and untabbable until heading completion",
    );
    assert.ok(
      (await page.locator(`${root} .handwriting-word`).count()) > 0,
      "the recent heading is writing while its entries remain pending",
    );
    for (const selector of [".entry-title"])
      assert.equal(
        (
          await presentation(
            page,
            `${root} li:first-child .entry-link ${selector}`,
          )
        ).visibility,
        "hidden",
        `${selector} stays hidden with its entry`,
      );
    assert.equal(
      (await presentation(page, `${root} .all-writing`)).visibility,
      "hidden",
    );
    assert.equal(await allWriting.evaluate((link) => link.inert), true);

    let stagger;
    for (let step = 0; step < 80; step++) {
      await page.clock.runFor(30);
      stagger = await entryStates(page);
      if (
        stagger[0].opacity > stagger[1].opacity &&
        stagger[1].opacity >= stagger[2].opacity
      )
        break;
    }
    assert.ok(
      stagger[0].opacity > stagger[1].opacity &&
        stagger[1].opacity >= stagger[2].opacity,
      `complete entries reveal in order after writing: ${JSON.stringify(stagger)}`,
    );
    assert.equal(
      (await presentation(page, `${root} .all-writing`)).opacity,
      0,
      "All writing waits for the complete entry sequence",
    );

    let linkOpacity = 0;
    for (let step = 0; step < 50; step++) {
      await page.clock.runFor(30);
      linkOpacity = (await presentation(page, `${root} .all-writing`)).opacity;
      if (linkOpacity > 0) break;
    }
    assert.ok(linkOpacity > 0, "All writing begins after entries");
    assert.ok(
      (await entryStates(page)).every((entry) => entry.opacity > 0.99),
      "All writing starts only after every entry finishes revealing",
    );
    await page.clock.runFor(600);
    const revealed = await entryStates(page);
    assert.ok(
      revealed.every(
        (entry) =>
          entry.visibility === "visible" &&
          entry.opacity > 0.99 &&
          !entry.inert,
      ),
    );
    assert.ok(
      (await presentation(page, `${root} .all-writing`)).opacity > 0.99,
    );
    assert.equal(await allWriting.evaluate((link) => link.inert), false);
    await entries.last().focus();
    await page.keyboard.press("Tab");
    assert.equal(
      await allWriting.evaluate((link) => document.activeElement === link),
      true,
      "All writing follows the last entry in keyboard order",
    );
  },
);

await withPage(
  { route: "/tests/recent-writing/", reducedMotion: "reduce" },
  async (page) => {
    assert.equal(
      await page.locator(root).count(),
      1,
      "empty posts omit their complete component wrapper",
    );
    assert.equal(await page.locator("#empty-recent-heading").count(), 0);
    assert.equal(await page.locator(`${root} .all-writing`).count(), 1);
    assert.equal(await page.locator(`${root} .entry-link`).count(), 3);
  },
);
