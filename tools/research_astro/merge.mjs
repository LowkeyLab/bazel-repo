import { createRequire } from "node:module";
import { readFileSync, writeFileSync, mkdirSync, readdirSync } from "node:fs";
import { resolve, basename } from "node:path";
import { pathToFileURL } from "node:url";
import assert from "node:assert/strict";
import { isDeepStrictEqual } from "node:util";
const require = createRequire(import.meta.url);
const ar = createRequire(require.resolve("astro/package.json"));
const compiler = await import(ar.resolve("@astrojs/compiler-rs"));
const cr = createRequire(
  require.resolve("@vitest/coverage-v8", {
    paths: [resolve("angular/node_modules")],
  }),
);
const { convert } = await import(cr.resolve("ast-v8-to-istanbul"));
const { createCoverageMap } = await import(
  cr.resolve("@vitest/istanbul-lib-coverage")
);
const [serverDir, browserDir, output] = process.argv
  .slice(2)
  .map((value) => resolve(value));
mkdirSync(output, { recursive: true });
const read = (path) => JSON.parse(readFileSync(path, "utf8"));
const save = (name, value) =>
  writeFileSync(
    resolve(output, name),
    typeof value === "string" ? value : JSON.stringify(value, null, 2),
  );
const fixtures = resolve("tools/research_astro/fixtures");
async function ast(code) {
  const result = await compiler.parse(`<script>${code}</script>`);
  const program = result.ast.body[0].children[0].program;
  function shift(value) {
    if (!value || typeof value !== "object") return;
    for (const [key, child] of Object.entries(value)) {
      if ((key === "start" || key === "end") && typeof child === "number")
        value[key] -= 8;
      else shift(child);
    }
  }
  shift(program);
  return program;
}
const canonical = (value) =>
  value.includes("book_smartz/web/submitting.js")
    ? "book_smartz/web/submitting.js"
    : "tools/research_astro/fixtures/" + basename(value);
function normalize(files) {
  const result = {};
  for (const [path, value] of Object.entries(files)) {
    const key = canonical(path);
    result[key] = { ...value, path: key };
  }
  return result;
}
const baseline = createCoverageMap({});
for (const files of Object.values(read(resolve(serverDir, "baseline.json"))))
  baseline.merge(normalize(files));
const measuredServer = createCoverageMap({});
for (const files of Object.values(
  read(resolve(serverDir, "server-measured.json")),
))
  measuredServer.merge(normalize(files));
for (const filename of [
  "node-helper.js",
  "browser-helper.js",
  "disconnected.js",
  "evaluated.js",
]) {
  const file = resolve(fixtures, filename),
    code = readFileSync(file, "utf8");
  baseline.merge(
    normalize(
      await convert({
        code,
        ast: await ast(code),
        coverage: {
          url: pathToFileURL(file).href,
          functions: [
            {
              functionName: "",
              isBlockCoverage: true,
              ranges: [{ startOffset: 0, endOffset: code.length, count: 0 }],
            },
          ],
        },
      }),
    ),
  );
}
const serverProfile = read(resolve(serverDir, "server-profile.json"));
const helperProfile = serverProfile.result.find((entry) =>
  entry.url.endsWith("/generated/node-helper.js"),
);
if (!helperProfile) throw Error("Required Node helper profile is absent.");
measuredServer.merge(
  normalize(
    await convert({
      code: readFileSync(resolve(fixtures, "node-helper.js"), "utf8"),
      ast: await ast(readFileSync(resolve(fixtures, "node-helper.js"), "utf8")),
      coverage: helperProfile,
    }),
  ),
);
const browserMap = createCoverageMap({});
const virtual = createCoverageMap({});
for (const interval of ["astro", "application"]) {
  assert.equal(
    read(resolve(browserDir, `${interval}-functional.json`)).passed,
    true,
  );
  assert.equal(
    read(resolve(browserDir, `${interval}-completion.json`)).completed,
    true,
  );
  const profile = read(resolve(browserDir, `${interval}-profile.json`));
  assert.ok(Array.isArray(profile.result) && profile.result.length > 0);
  for (const entry of profile.result) {
    const code = readFileSync(resolve(browserDir, entry.textFile), "utf8");
    let sourceMap,
      file,
      shift = 0;
    if (entry.url.endsWith("/application/submitting.js")) {
      file = resolve("book_smartz/web/submitting.js");
      assert.equal(code, readFileSync(file, "utf8"));
    } else if (entry.url === "research-evaluated.js") {
      file = resolve(fixtures, "evaluated.js");
      assert.equal(
        code,
        readFileSync(file, "utf8") + "\n//# sourceURL=research-evaluated.js",
      );
    } else if (entry.parsed?.sourceMapURL) {
      const mapName = basename(entry.parsed.sourceMapURL);
      sourceMap = read(resolve(browserDir, mapName));
      sourceMap.sources = sourceMap.sources.map((source) =>
        resolve(
          fixtures,
          basename(source.split("?")[0]) +
            (source.includes("?") ? ".virtual.js" : ""),
        ),
      );
      file = resolve(fixtures, "processed-generated.js");
    } else if (code.includes("window.inlineResult")) {
      file = resolve(fixtures, "Main.astro");
      const original = readFileSync(file, "utf8"),
        offset = original.indexOf(code);
      assert.ok(
        offset >= 0 && original.indexOf(code, offset + 1) < 0,
        "Inline block identity must be unique.",
      );
      shift = original.slice(0, offset).split("\n").length - 1;
    } else continue;
    const converted = await convert({
      code,
      ast: await ast(code),
      sourceMap,
      coverage: { ...entry, url: pathToFileURL(file).href },
    });
    for (const [path, value] of Object.entries(converted)) {
      if (shift) {
        function move(node) {
          if (!node || typeof node !== "object") return;
          for (const [key, child] of Object.entries(node)) {
            if (key === "line" && typeof child === "number") node[key] += shift;
            else move(child);
          }
        }
        move(value.statementMap);
        move(value.fnMap);
        move(value.branchMap);
      }
      (path.endsWith(".virtual.js") ? virtual : browserMap).merge(
        normalize({ [path]: value }),
      );
    }
  }
}
function lcov(map) {
  let text = "";
  for (const path of map.files().sort()) {
    const file = map.fileCoverageFor(path);
    text += `TN:research\nSF:${path}\n`;
    for (const [line, count] of Object.entries(file.getLineCoverage()).sort(
      (a, b) => Number(a[0]) - Number(b[0]),
    ))
      text += `DA:${line},${count}\n`;
    for (const [id, branch] of Object.entries(file.branchMap))
      for (const [arm, count] of file.b[id].entries())
        text += `BRDA:${branch.line},${id},${arm},${count}\n`;
    text += "end_of_record\n";
  }
  return text;
}
const measured = createCoverageMap({});
measured.merge(JSON.parse(JSON.stringify(measuredServer)));
measured.merge(JSON.parse(JSON.stringify(browserMap)));
const zeroFirst = createCoverageMap({});
zeroFirst.merge(JSON.parse(JSON.stringify(baseline)));
zeroFirst.merge(JSON.parse(JSON.stringify(measured)));
const zeroLast = createCoverageMap({});
zeroLast.merge(JSON.parse(JSON.stringify(measured)));
zeroLast.merge(JSON.parse(JSON.stringify(baseline)));
function summary(map) {
  return map
    .files()
    .sort()
    .map((path) => {
      const f = map.fileCoverageFor(path);
      return [
        path,
        f.getLineCoverage(),
        Object.entries(f.branchMap)
          .map(([id, b]) => [b.locations, f.b[id]])
          .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b))),
      ];
    });
}
const bothOrdersEqual = isDeepStrictEqual(
  summary(zeroFirst),
  summary(zeroLast),
);
save("merge-order-summary.json", {
  zeroFirst: summary(zeroFirst),
  zeroLast: summary(zeroLast),
});
for (const [name, map] of [
  ["baseline", baseline],
  ["server", measuredServer],
  ["browser", browserMap],
  ["virtual-unresolved", virtual],
  ["combined-zero-first", zeroFirst],
  ["combined-zero-last", zeroLast],
]) {
  save(`${name}.json`, map.toJSON());
  save(`${name}.lcov`, lcov(map));
}
save("merge-result.json", {
  bothOrdersEqual,
  unresolvedVirtualFiles: virtual.files(),
  completeOriginalAstroCoverage: false,
});
console.log(
  JSON.stringify({
    files: zeroFirst.files(),
    virtual: virtual.files(),
    bothOrdersEqual,
  }),
);
