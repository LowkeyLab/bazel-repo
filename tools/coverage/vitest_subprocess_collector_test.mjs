import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

const nodeBinary = process.env.JS_BINARY__NODE_BINARY;
assert.ok(nodeBinary);
const vitest = new URL(
  "../../angular/node_modules/vitest/dist/node.js",
  import.meta.url,
).href;
const scenarios = [
  { name: "physical", ambiguous: false, preserveMain: false, measured: true },
  { name: "ambiguous", ambiguous: true, preserveMain: false, measured: false },
  { name: "logical", ambiguous: true, preserveMain: true, measured: true },
];

for (const scenario of scenarios) {
  const temporary = fs.mkdtempSync(
    path.join(process.env.TEST_TMPDIR ?? os.tmpdir(), "vitest-child-"),
  );
  try {
    const project = path.join(temporary, "project");
    const physical = path.join(temporary, "physical");
    const reports = path.join(temporary, "reports");
    const profiles = path.join(
      process.env.TEST_UNDECLARED_OUTPUTS_DIR ?? temporary,
      scenario.name,
    );
    fs.mkdirSync(project);
    fs.mkdirSync(physical);
    fs.mkdirSync(profiles);
    fs.writeFileSync(
      path.join(physical, "child.mjs"),
      'if (process.argv.length > 1) {\n  process.stdout.write("23");\n}\n',
    );
    fs.writeFileSync(
      path.join(physical, "foreign.mjs"),
      'process.stdout.write("29");\n',
    );
    fs.symlinkSync(
      path.join(physical, "child.mjs"),
      path.join(project, "child.mjs"),
    );
    if (scenario.ambiguous)
      fs.symlinkSync(
        path.join(physical, "child.mjs"),
        path.join(project, "alias.mjs"),
      );
    fs.writeFileSync(
      path.join(project, "parent.mjs"),
      "export function parent() { return 7; }\n",
    );
    fs.writeFileSync(
      path.join(project, "unused.mjs"),
      "export function unused() { return 3; }\n",
    );
    const argumentsForChild = [path.join(project, "child.mjs")];
    if (scenario.preserveMain)
      argumentsForChild.unshift("--preserve-symlinks-main");
    fs.writeFileSync(
      path.join(project, "test.spec.mjs"),
      `import { spawnSync } from 'node:child_process';
import { copyFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { parent } from './parent.mjs';
test('the child executes', () => {
  expect(parent()).toBe(7);
  const child = spawnSync(${JSON.stringify(nodeBinary)}, ${JSON.stringify(argumentsForChild)}, { encoding: 'utf8' });
  expect(child.status).toBe(0);
  expect(child.stdout).toBe('23');
  const foreign = spawnSync(${JSON.stringify(nodeBinary)}, [${JSON.stringify(path.join(physical, "foreign.mjs"))}], { encoding: 'utf8' });
  expect(foreign.status).toBe(0);
  expect(foreign.stdout).toBe('29');
  const directory = process.env.NODE_V8_COVERAGE;
  for (const file of readdirSync(directory)) copyFileSync(join(directory, file), join(${JSON.stringify(profiles)}, file));
});
`,
    );
    const runner = path.join(temporary, "runner.mjs");
    fs.writeFileSync(
      runner,
      `import { startVitest } from ${JSON.stringify(vitest)};
const context = await startVitest('test', [], ${JSON.stringify({ root: project, configFile: false, watch: false, globals: true, pool: "forks", maxWorkers: 1, fileParallelism: false, include: ["test.spec.mjs"], coverage: { enabled: true, provider: "v8", autoAttachSubprocess: true, reportsDirectory: reports, reporter: ["lcovonly"], include: ["*.mjs"], exclude: ["**/*.spec.mjs"] } })});
if (!context) throw new Error('Vitest did not start.');
await context.close();
`,
    );
    const environment = { ...process.env, NODE_V8_COVERAGE: "" };
    delete environment.NODE_OPTIONS;
    const result = spawnSync(nodeBinary, [runner], {
      cwd: project,
      env: environment,
      encoding: "utf8",
      timeout: 30_000,
    });
    assert.equal(result.status, 0, result.stdout + result.stderr);
    const originalUrl = pathToFileURL(
      path.join(scenario.preserveMain ? project : physical, "child.mjs"),
    ).href;
    const scripts = fs
      .readdirSync(profiles)
      .flatMap(
        (file) =>
          JSON.parse(fs.readFileSync(path.join(profiles, file), "utf8")).result,
      );
    assert.ok(
      scripts.some(
        (script) =>
          script.url === originalUrl &&
          script.functions.some((fn) =>
            fn.ranges.some((range) => range.count > 0),
          ),
      ),
      "Raw child execution is absent.",
    );
    const records = new Map();
    for (const record of fs
      .readFileSync(path.join(reports, "lcov.info"), "utf8")
      .split("end_of_record")) {
      if (!record.trim()) continue;
      const source = record
        .split("\n")
        .find((line) => line.startsWith("SF:"))
        ?.slice(3);
      assert.ok(source);
      assert.ok(!records.has(source), `Duplicate source: ${source}`);
      records.set(
        source,
        record
          .split("\n")
          .filter((line) => line.startsWith("DA:"))
          .map((line) => Number(line.split(",")[1])),
      );
    }
    assert.ok(
      records.get("parent.mjs")?.some((count) => count > 0),
      "Parent hits are absent.",
    );
    assert.ok(records.get("unused.mjs")?.length, "Unused baseline is absent.");
    assert.ok(
      records.get("unused.mjs").every((count) => count === 0),
      "Unused baseline has hits.",
    );
    assert.equal(
      records.get("child.mjs")?.some((count) => count > 0) ?? false,
      scenario.measured,
      "Child attribution differs.",
    );
    if (scenario.ambiguous)
      assert.ok(
        !records.get("alias.mjs")?.some((count) => count > 0),
        "An ambiguous alias borrowed hits.",
      );
    assert.deepEqual(
      [...records.keys()].sort(),
      scenario.ambiguous
        ? ["alias.mjs", "child.mjs", "parent.mjs", "unused.mjs"]
        : ["child.mjs", "parent.mjs", "unused.mjs"],
    );
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
}
