/* global process, URL, console, document, innerWidth */
// Throwaway fixture server. It exposes only declared prototype assets.
import { createServer } from "node:http";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const files = new Map([
  ["/", ["prototype.html", "text/html"]],
  ["/ranking", ["prototype.html", "text/html"]],
  ["/prototype_styles.css", ["prototype_styles.css", "text/css"]],
  ...["literata.ttf", "source-sans-3.ttf", "dune.jpg", "left-hand.jpg"].map(
    (name) => [
      `/prototype-assets/${name}`,
      [
        `prototype-assets/${name}`,
        name.endsWith(".ttf") ? "font/ttf" : "image/jpeg",
      ],
    ],
  ),
]);
const server = createServer(async (request, response) => {
  const match = files.get(new URL(request.url, "http://localhost").pathname);
  if (!match) {
    response.writeHead(404).end();
    return;
  }
  try {
    const data = await readFile(
      fileURLToPath(new URL(match[0], import.meta.url)),
    );
    response
      .writeHead(200, { "Content-Type": match[1], "Cache-Control": "no-store" })
      .end(data);
  } catch {
    response.writeHead(500).end("Prototype asset is missing.");
  }
});
const asset = (name) => readFile(fileURLToPath(new URL(name, import.meta.url)));
if (process.argv[2] === "--export") {
  let html = (await asset("prototype.html")).toString();
  const css = (await asset("prototype_styles.css")).toString();
  html = html.replace(
    '<link rel="stylesheet" href="/prototype_styles.css" />',
    `<style>${css}</style>`,
  );
  for (const [path, [name, mime]] of files) {
    if (!path.startsWith("/prototype-assets/")) continue;
    html = html.replaceAll(
      path,
      `data:${mime};base64,${(await asset(name)).toString("base64")}`,
    );
  }
  await writeFile(process.argv[3], html);
  console.log(`Standalone prototype: ${process.argv[3]}`);
} else {
  await new Promise((resolve) => server.listen(2615, "127.0.0.1", resolve));
  console.log("Prototype: http://127.0.0.1:2615/ranking?variant=A");
  if (process.argv[2] === "--review") {
    const { chromium } = await import("playwright");
    const browser = await chromium.launch({
      executablePath: process.env.CHROMIUM_EXECUTABLE,
      args: ["--no-sandbox"],
    });
    const output = process.argv[3];
    await mkdir(output, { recursive: true });
    const findings = [];
    const errors = [];
    try {
      for (const width of [390, 1440]) {
        const page = await browser.newPage({
          viewport: { width, height: 1000 },
          reducedMotion: "reduce",
        });
        page.on("pageerror", (error) => errors.push(error.message));
        for (const variant of ["A", "B", "C"]) {
          for (const scene of [
            "comparison",
            "signin",
            "search",
            "selection",
            "submitting",
            "confirmed",
            "paused",
            "ranking",
            "empty",
            "noresults",
            "error",
            "checking",
            "rejected",
            "stale",
            "expired",
          ]) {
            await page.goto(
              `http://127.0.0.1:2615/ranking?variant=${variant}&scene=${scene}`,
            );
            await page.evaluate(() => document.fonts.ready);
            const overflow = await page.evaluate(
              () => document.documentElement.scrollWidth > innerWidth,
            );
            const missingImages = await page
              .locator("img")
              .evaluateAll(
                (images) =>
                  images.filter(
                    (image) => !image.complete || image.naturalWidth === 0,
                  ).length,
              );
            findings.push({ width, variant, scene, overflow, missingImages });
            if (["comparison", "search", "checking"].includes(scene))
              await page.screenshot({
                path: `${output}/${variant}-${scene}-${width}.png`,
                fullPage: true,
              });
          }
        }
        await page.goto("http://127.0.0.1:2615/ranking?variant=A");
        await page
          .getByRole("button", { name: "Choose Piranesi", exact: true })
          .click();
        if (await page.getByRole("status").count())
          throw new Error("Submission displayed confirmed success.");
        await page
          .getByRole("button", { name: "Simulate server confirmation" })
          .click();
        if (
          !(await page.getByRole("status").innerText()).includes(
            "Preference saved",
          )
        )
          throw new Error("Confirmation feedback is missing.");
        await page.getByRole("button", { name: "Pause for now" }).click();
        await page
          .getByRole("button", { name: "Simulate server confirmation" })
          .click();
        await page.getByRole("button", { name: "Resume placement" }).click();
        await page
          .getByRole("button", { name: "Simulate server confirmation" })
          .click();
        await page.locator("body").click({ position: { x: 1, y: 1 } });
        await page.keyboard.press("ArrowRight");
        if (!page.url().includes("variant=B"))
          throw new Error("Keyboard layout selection failed.");
        await page.close();
      }
      await writeFile(
        `${output}/review.json`,
        JSON.stringify({ findings, errors }, null, 2),
      );
      if (
        errors.length ||
        findings.some((item) => item.overflow || item.missingImages)
      )
        throw new Error(
          "Review found browser errors, overflow, or missing images.",
        );
      console.log(
        `Reviewed ${findings.length} layouts and screens. Screenshots: ${output}`,
      );
    } finally {
      await browser.close();
      server.close();
    }
  }
}
