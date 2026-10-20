import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
  rmSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const here = dirname(fileURLToPath(import.meta.url));
const converter = resolve(process.argv[2]);
const out = process.env.TEST_UNDECLARED_OUTPUTS_DIR;
mkdirSync(out, { recursive: true });
const tmp = process.env.TEST_TMPDIR;
const results = [];
const sourceRoot = join(tmp, "runfiles", "_main");
const relative = "tools/coverage/node_research";
const sources = join(sourceRoot, relative);
mkdirSync(sources, { recursive: true });
for (const f of readdirSync(here))
  if (/\.(ts|js|mjs|map)$/.test(f)) cpSync(join(here, f), join(sources, f));
const raw = join(tmp, "raw-profiles");
mkdirSync(raw);
const child = spawnSync(
  process.env.JS_BINARY__NODE_BINARY,
  [join(sources, "native.test.mjs")],
  {
    env: {
      ...process.env,
      NODE_OPTIONS: "",
      COVERAGE_MANIFEST: "",
      NODE_V8_COVERAGE: raw,
      TEST_UNDECLARED_OUTPUTS_DIR: join(out, "functional"),
    },
    encoding: "utf8",
  },
);
assert.equal(child.status, 0, child.stderr);
cpSync(raw, join(out, "raw-profiles"), { recursive: true });
for (const mode of [
  "original-manifest",
  "emitted-manifest",
  "missing-profile",
  "corrupt-profile",
  "missing-runfile",
]) {
  const profiles = join(tmp, mode);
  cpSync(raw, profiles, { recursive: true });
  const scripts = ["production", "helper", "child_only", "disconnected"];
  const entries = [
    `${relative}/native.test.mjs`,
    `${relative}/child.mjs`,
    ...scripts.map(
      (f) => `${relative}/${f}.${mode === "original-manifest" ? "ts" : "js"}`,
    ),
  ];
  if (mode === "missing-profile")
    for (const f of readdirSync(profiles)) rmSync(join(profiles, f));
  if (mode === "corrupt-profile")
    for (const f of readdirSync(profiles))
      writeFileSync(join(profiles, f), "{corrupt");
  if (mode === "missing-runfile")
    entries.push(`${relative}/absent-maintained.js`);
  const manifest = join(tmp, `${mode}.manifest`);
  writeFileSync(manifest, entries.join("\n") + "\n");
  const r = spawnSync(process.env.JS_BINARY__NODE_BINARY, [converter], {
    env: {
      ...process.env,
      NODE_OPTIONS: "",
      NODE_V8_COVERAGE: "",
      COVERAGE_DIR: profiles,
      COVERAGE_MANIFEST: manifest,
      JS_COVERAGE__RUNFILES: join(tmp, "runfiles"),
      TEST_WORKSPACE: "_main",
    },
    encoding: "utf8",
  });
  const destination = join(out, mode);
  mkdirSync(destination);
  writeFileSync(join(destination, "stdout"), r.stdout ?? "");
  writeFileSync(join(destination, "stderr"), r.stderr ?? "");
  cpSync(manifest, join(destination, "manifest.txt"));
  const report = join(profiles, "_rules_js_report.lcov");
  const text = existsSync(report) ? readFileSync(report, "utf8") : "";
  if (existsSync(report)) cpSync(report, join(destination, "report.lcov"));
  const blocks = text
    .split("end_of_record")
    .map((b) => b.slice(b.indexOf("SF:")).trim())
    .filter(Boolean);
  const expected = [
    "production.ts",
    "helper.ts",
    "child_only.ts",
    "disconnected.ts",
    "native.test.mjs",
  ];
  const missing = expected.filter(
    (f) => !blocks.some((b) => b.startsWith(`SF:${relative}/${f}\n`)),
  );
  const positive = Object.fromEntries(
    expected.map((f) => [
      f,
      blocks.some(
        (b) =>
          b.startsWith(`SF:${relative}/${f}\n`) && /^DA:\d+,[1-9]\d*$/m.test(b),
      ),
    ]),
  );
  const validationErrors = [...missing.map((f) => `missing-source:${f}`)];
  for (const f of [
    "production.ts",
    "helper.ts",
    "child_only.ts",
    "native.test.mjs",
  ])
    if (!positive[f]) validationErrors.push(`missing-hit:${f}`);
  if (
    mode === "missing-runfile" &&
    !existsSync(join(sources, "absent-maintained.js"))
  )
    validationErrors.push("missing-runfile:absent-maintained.js");
  results.push({
    mode,
    functionalExit: child.status,
    converterExit: r.status,
    missing,
    positive,
    validationErrors,
  });
}
cpSync(sources, join(out, "originals-and-emitted"), { recursive: true });
writeFileSync(join(out, "results.json"), JSON.stringify(results, null, 2));
console.log(JSON.stringify(results, null, 2));
