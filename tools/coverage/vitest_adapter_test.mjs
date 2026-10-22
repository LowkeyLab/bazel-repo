import assert from "node:assert/strict";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { test } from "node:test";

const originalTemporary = process.env.TEST_TMPDIR;
const originalOutput = process.env.COVERAGE_OUTPUT_FILE;
const temporary = mkdtempSync(join(originalTemporary, "vitest-adapter-"));
process.env.TEST_TMPDIR = temporary;
process.env.COVERAGE_OUTPUT_FILE = join(temporary, "result.lcov");
const adapter = await import("../../angular/bzl/vitest-coverage.mjs");
adapter.setup();
const input = join(adapter.reportsDirectory, "lcov.info");
const output = process.env.COVERAGE_OUTPUT_FILE;
const valid =
  "SF:projects/example/src/value.ts\nDA:2,0\nDA:3,7\nLF:2\nLH:1\nBRDA:2,0,0,0\nBRDA:2,0,1,7\nBRF:2\nBRH:1\nend_of_record\n";

try {
  await test("the adapter retains zero counts, measured counts, and native branches", () => {
    writeFileSync(input, valid);
    adapter.teardown();
    assert.equal(
      readFileSync(output, "utf8"),
      valid.replace("SF:projects/", "SF:angular/projects/"),
    );
  });
  await test("the adapter rejects a missing required report", () => {
    rmSync(input);
    assert.throws(() => adapter.teardown(), /ENOENT/);
  });
  for (const [name, content] of [
    ["empty output", ""],
    ["incomplete output", valid.replace("end_of_record", "")],
    ["missing source", valid.replace("SF:projects/example/src/value.ts\n", "")],
    ["duplicate source", "SF:projects/other.ts\n" + valid],
    ["corrupt line", valid.replace("DA:2,0", "DA:2,broken")],
    ["duplicate line", valid.replace("DA:2,0", "DA:2,0\nDA:2,0")],
    ["incomplete line total", valid.replace("LF:2", "LF:3")],
    ["incomplete hit total", valid.replace("LH:1", "LH:2")],
    ["corrupt branch", valid.replace("BRDA:2,0,0,0", "BRDA:2,0,0,broken")],
    ["incomplete branch total", valid.replace("BRF:2", "BRF:3")],
    ["incomplete branch hits", valid.replace("BRH:1", "BRH:2")],
    [
      "escaping source",
      valid.replace("projects/example/src/value.ts", "../private.ts"),
    ],
  ]) {
    await test(`the adapter rejects ${name} and preserves the prior report`, () => {
      const previous = readFileSync(output, "utf8");
      writeFileSync(input, content);
      assert.throws(() => adapter.teardown());
      assert.equal(readFileSync(output, "utf8"), previous);
    });
  }
  await test("setup removes a stale report", () => {
    writeFileSync(input, valid);
    adapter.setup();
    assert.throws(() => adapter.teardown(), /ENOENT/);
  });
  await test("ordinary execution requires no coverage report", () => {
    delete process.env.COVERAGE_OUTPUT_FILE;
    rmSync(input, { force: true });
    assert.doesNotThrow(() => adapter.teardown());
  });
} finally {
  process.env.TEST_TMPDIR = originalTemporary;
  if (originalOutput === undefined) delete process.env.COVERAGE_OUTPUT_FILE;
  else process.env.COVERAGE_OUTPUT_FILE = originalOutput;
  mkdirSync(temporary, { recursive: true });
}
