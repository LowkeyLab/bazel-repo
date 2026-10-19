import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { choose } from "./production.js";
import { twice } from "./helper.js";
const here = dirname(fileURLToPath(import.meta.url));
assert.equal(choose(1), 2);
assert.equal(twice(3), 6);
const child = spawnSync(
  process.execPath,
  ["--preserve-symlinks", join(here, "child.mjs")],
  { encoding: "utf8" },
);
assert.equal(child.status, 0, child.stderr);
const out = process.env.TEST_UNDECLARED_OUTPUTS_DIR;
mkdirSync(out, { recursive: true });
writeFileSync(
  join(out, "environment.json"),
  JSON.stringify(
    {
      node: process.version,
      cwd: process.cwd(),
      manifest: process.env.COVERAGE_MANIFEST,
      profileRoot: process.env.NODE_V8_COVERAGE,
      coverage: process.env.COVERAGE_OUTPUT_FILE,
      childStatus: child.status,
    },
    null,
    2,
  ),
);
if (process.env.COVERAGE_MANIFEST)
  copyFileSync(process.env.COVERAGE_MANIFEST, join(out, "manifest.txt"));
for (const file of readdirSync(here))
  if (/\.(js|mjs|ts|map)$/.test(file))
    copyFileSync(join(here, file), join(out, file));
if (process.env.NODE_V8_COVERAGE && existsSync(process.env.NODE_V8_COVERAGE))
  cpSync(process.env.NODE_V8_COVERAGE, join(out, "profiles-at-test-end"), {
    recursive: true,
  });
