import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, relative, resolve, sep } from "node:path";
import type { Page } from "playwright";
import v8ToIstanbul from "v8-to-istanbul";
import libCoverage from "istanbul-lib-coverage";
import libReport from "istanbul-lib-report";
import reports from "istanbul-reports";

// Playwright supplies raw Chromium ranges. The maintained converter supplies
// mapping and executable records. This adapter adds no coverage identities.
const pages: string[] = [];
let indexWrite = Promise.resolve();

export async function browserCoverage(
  page: Page,
  directory: string,
  javaScriptEnabled = true,
) {
  if (!process.env.COVERAGE_DIR) return async () => {};
  const outputs = process.env.TEST_UNDECLARED_OUTPUTS_DIR;
  assert.ok(outputs, "Browser coverage requires a declared output directory.");
  const id = randomUUID();
  const destination = join(outputs, "browser-coverage", id);
  await mkdir(destination, { recursive: true });
  pages.push(id);
  indexWrite = indexWrite.then(() =>
    writeFile(
      join(outputs, "browser-coverage/pages.json"),
      JSON.stringify(pages),
    ),
  );
  await indexWrite;
  if (javaScriptEnabled) await page.coverage.startJSCoverage();
  return async () => {
    const raw = javaScriptEnabled ? await page.coverage.stopJSCoverage() : [];
    const profile = JSON.stringify(raw);
    await writeFile(join(destination, "profile.json"), profile);
    const coverage = libCoverage.createCoverageMap({});
    const inputs = [];
    for (const entry of raw) {
      const url = new URL(entry.url);
      // Anonymous evaluation scripts and other origins are outside this site.
      if (url.hostname !== "127.0.0.1" || !url.pathname.endsWith(".js"))
        continue;
      const script = resolve(directory, "." + decodeURIComponent(url.pathname));
      const local = relative(directory, script);
      assert.ok(!local.startsWith(".." + sep) && local !== "..");
      const source = await readFile(script, "utf8");
      assert.equal(
        entry.source,
        source,
        "Coverage must match the built script.",
      );
      const mapName = source.match(/[#@] sourceMappingURL=([^\s]+)/)?.[1];
      assert.ok(mapName, `Required browser source map is absent: ${local}`);
      assert.ok(!mapName.includes(":") && !mapName.startsWith("/"));
      const mapPath = resolve(dirname(script), mapName);
      assert.ok(relative(directory, mapPath).split(sep)[0] !== "..");
      const map = JSON.parse(await readFile(mapPath, "utf8"));
      assert.equal(map.version, 3);
      assert.ok(Array.isArray(map.sources) && map.sources.length > 0);
      assert.equal(typeof map.mappings, "string");
      assert.ok(Array.isArray(map.sourcesContent));
      assert.equal(map.sourcesContent.length, map.sources.length);
      assert.ok(
        map.sourcesContent.every((value: unknown) => typeof value === "string"),
      );
      const converter = v8ToIstanbul(script, 0, { source });
      await converter.load();
      converter.applyCoverage(entry.functions);
      coverage.merge(converter.toIstanbul());
      const index = inputs.length;
      await writeFile(join(destination, `${index}.js`), source);
      await writeFile(join(destination, `${index}.map`), JSON.stringify(map));
      inputs.push({
        script,
        sources: map.sources,
        sourcesContent: map.sourcesContent,
        map_sha256: createHash("sha256")
          .update(JSON.stringify(map))
          .digest("hex"),
      });
    }
    reports
      .create("lcovonly", { file: "report.lcov" })
      .execute(
        libReport.createContext({ dir: destination, coverageMap: coverage }),
      );
    await writeFile(
      join(destination, "receipt.json"),
      JSON.stringify({
        schema_version: 1,
        url: page.url(),
        scripts: inputs,
        sources: coverage.files().map((file) => relative(process.cwd(), file)),
        profile_sha256: createHash("sha256").update(profile).digest("hex"),
        report_sha256: createHash("sha256")
          .update(await readFile(join(destination, "report.lcov")))
          .digest("hex"),
      }),
    );
  };
}
