import { createRequire } from "node:module";
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  copyFileSync,
  symlinkSync,
} from "node:fs";
import { resolve, dirname } from "node:path";
import { pathToFileURL } from "node:url";
import { Session } from "node:inspector/promises";
const require = createRequire(import.meta.url);
const ar = createRequire(require.resolve("astro/package.json"));
const compiler = await import(ar.resolve("@astrojs/compiler-rs"));
const { experimental_AstroContainer: Container } =
  await import("astro/container");
const cr = createRequire(
  require.resolve("@vitest/coverage-v8", {
    paths: [resolve("angular/node_modules")],
  }),
);
const { convert } = await import(cr.resolve("ast-v8-to-istanbul"));
const output = resolve(process.argv[2]);
mkdirSync(output, { recursive: true });
const save = (name, value) =>
  writeFileSync(
    resolve(output, name),
    typeof value === "string" ? value : JSON.stringify(value, null, 2),
  );
const fixtures = resolve("tools/research_astro/fixtures");
const generated = resolve(output, "generated");
mkdirSync(generated, { recursive: true });
try {
  symlinkSync(
    resolve("node_modules"),
    resolve(generated, "node_modules"),
    "dir",
  );
} catch (error) {
  if (error.code !== "EEXIST") throw error;
}
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
const entries = [];
for (const name of ["Main", "Disconnected", "TestOnly"]) {
  const filename = resolve(fixtures, `${name}.astro`);
  const source = readFileSync(filename, "utf8");
  const result = await compiler.transform(source, {
    filename,
    sourcemap: "external",
    internalURL: "astro/compiler-runtime",
    resultScopedSlot: true,
    resolvePath: (specifier) => resolve(fixtures, specifier),
  });
  const file = resolve(generated, `${name}.mjs`);
  writeFileSync(file, result.code);
  save(`${name}.compiler.json`, result);
  save(`${name}.ast.json`, await compiler.parse(source));
  entries.push({ name, file, code: result.code, map: JSON.parse(result.map) });
}
for (const name of ["node-helper.js", "browser-helper.js", "disconnected.js"])
  copyFileSync(resolve(fixtures, name), resolve(generated, name));
const baseline = {};
for (const entry of entries) {
  baseline[entry.name] = await convert({
    code: entry.code,
    ast: await ast(entry.code),
    sourceMap: entry.map,
    coverage: {
      url: pathToFileURL(entry.file).href,
      functions: [
        {
          functionName: "",
          isBlockCoverage: true,
          ranges: [{ startOffset: 0, endOffset: entry.code.length, count: 0 }],
        },
      ],
    },
  });
}
save("baseline.json", baseline);
const session = new Session();
session.connect();
await session.post("Profiler.enable");
await session.post("Profiler.startPreciseCoverage", {
  callCount: true,
  detailed: true,
});
const component = (await import(pathToFileURL(entries[0].file).href)).default;
const testOnly = (await import(pathToFileURL(entries[2].file).href)).default;
const container = await Container.create();
const rendered = [];
for (const enabled of [true, false])
  rendered.push(
    await container.renderToString(component, { props: { enabled } }),
  );
rendered.push(
  await container.renderToString(testOnly, { props: { enabled: true } }),
);
if (
  !rendered[0].includes("server-yes") ||
  !rendered[1].includes("server-no") ||
  !rendered[2].includes("test-yes")
)
  throw Error("Fresh render assertion failed.");
save("rendered.json", rendered);
const profile = await session.post("Profiler.takePreciseCoverage");
await session.post("Profiler.stopPreciseCoverage");
session.disconnect();
save("server-profile.json", profile);
const measured = {};
for (const entry of entries) {
  const coverage = profile.result.find(
    (value) => value.url === pathToFileURL(entry.file).href,
  );
  if (coverage)
    measured[entry.name] = await convert({
      code: entry.code,
      ast: await ast(entry.code),
      sourceMap: entry.map,
      coverage,
    });
}
save("server-measured.json", measured);
save("server-completion.json", {
  functionalAssertions: true,
  profileComplete: true,
  baselineExecutedComponents: false,
});
console.log(
  JSON.stringify({
    baselineFiles: Object.keys(baseline),
    rendered: rendered.map((value) => value.length),
    measuredFiles: Object.keys(measured),
  }),
);
