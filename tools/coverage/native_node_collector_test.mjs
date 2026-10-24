import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

const nodeBinary = process.env.JS_BINARY__NODE_BINARY;
assert.ok(nodeBinary);

const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "native-child-"));
try {
  const output = path.join(temporary, "execroot", "bazel-out", "bin");
  const runfiles = path.join(temporary, "runfiles", "_main");
  const profiles = path.join(temporary, "profiles");
  for (const directory of [output, runfiles, profiles]) {
    fs.mkdirSync(directory, { recursive: true });
  }
  const sources = {
    "child.mjs":
      'if (process.argv.length > 1) {\n  process.stdout.write("23");\n}\n',
    "unused.mjs": "export function unused(flag) {\n  return flag ? 7 : 3;\n}\n",
    "parent.mjs": `import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
const child = spawnSync(process.execPath, [process.argv[2]], { encoding: 'utf8' });
assert.equal(child.status, 0);
assert.equal(child.stdout, '23');
`,
  };
  for (const [name, source] of Object.entries(sources)) {
    fs.writeFileSync(path.join(output, name), source);
    fs.symlinkSync(path.join(output, name), path.join(runfiles, name));
  }
  const environment = { ...process.env, NODE_V8_COVERAGE: profiles };
  delete environment.NODE_OPTIONS;
  // The launcher preserves the parent path. A plain Node child resolves its symlink.
  const execution = spawnSync(
    nodeBinary,
    [
      "--preserve-symlinks-main",
      path.join(runfiles, "parent.mjs"),
      path.join(runfiles, "child.mjs"),
    ],
    { env: environment, encoding: "utf8" },
  );
  assert.equal(execution.status, 0, execution.stderr);
  const childUrl = pathToFileURL(path.join(output, "child.mjs")).href;
  const raw = fs
    .readdirSync(profiles)
    .flatMap(
      (name) =>
        JSON.parse(fs.readFileSync(path.join(profiles, name), "utf8")).result,
    );
  assert.ok(
    raw.some(
      (script) =>
        script.url === childUrl &&
        script.functions.some((entry) =>
          entry.ranges.some((range) => range.count > 0),
        ),
    ),
  );
  const originals = new Map(
    fs
      .readdirSync(profiles)
      .map((name) => [name, fs.readFileSync(path.join(profiles, name))]),
  );
  const manifest = path.join(temporary, "manifest");
  fs.writeFileSync(manifest, Object.keys(sources).join("\n") + "\n");
  const collector = path.resolve(
    process.env.JS_BINARY__RUNFILES,
    process.env.JS_BINARY__WORKSPACE,
    process.env.NATIVE_JAVASCRIPT_COLLECTOR,
  );
  const collectorEnvironment = {
    ...process.env,
    COVERAGE_DIR: profiles,
    COVERAGE_MANIFEST: manifest,
    JS_COVERAGE__RUNFILES: path.dirname(runfiles),
    TEST_WORKSPACE: "_main",
  };
  delete collectorEnvironment.NODE_V8_COVERAGE;
  delete collectorEnvironment.NODE_OPTIONS;
  const collected = spawnSync(nodeBinary, [collector], {
    env: collectorEnvironment,
    encoding: "utf8",
  });
  assert.equal(collected.status, 0, collected.stderr);
  const records = new Map();
  for (const record of fs
    .readFileSync(path.join(profiles, "_rules_js_report.lcov"), "utf8")
    .split("end_of_record")) {
    if (!record.trim()) continue;
    const source = record
      .split("\n")
      .find((line) => line.startsWith("SF:"))
      .slice(3);
    assert.ok(!records.has(source), `Duplicate source: ${source}`);
    records.set(
      source,
      record
        .split("\n")
        .filter((line) => line.startsWith("DA:"))
        .map((line) => Number(line.split(",")[1])),
    );
  }
  assert.deepEqual([...records.keys()].sort(), Object.keys(sources).sort());
  assert.ok(
    records.get("parent.mjs").some((count) => count > 0),
    "Parent hits are absent.",
  );
  assert.ok(
    records.get("child.mjs").some((count) => count > 0),
    "Child hits are absent.",
  );
  assert.ok(records.get("unused.mjs").length > 0);
  assert.ok(records.get("unused.mjs").every((count) => count === 0));
  for (const [name, bytes] of originals) {
    assert.deepEqual(fs.readFileSync(path.join(profiles, name)), bytes);
  }
} finally {
  fs.rmSync(temporary, { recursive: true, force: true });
}
