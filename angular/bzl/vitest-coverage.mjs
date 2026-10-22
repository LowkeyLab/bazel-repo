import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, posix } from "node:path";
import process from "node:process";

export const reportsDirectory = join(
  process.env.TEST_TMPDIR ?? join(tmpdir(), `bazel-repo-vitest-${process.pid}`),
  ".vitest-coverage",
);

export const coverage = {
  reportsDirectory,
  autoAttachSubprocess: true,
};

export function setup() {
  mkdirSync(reportsDirectory, { recursive: true });
  rmSync(join(reportsDirectory, "lcov.info"), { force: true });
}

export function teardown() {
  const output = process.env.COVERAGE_OUTPUT_FILE;
  if (!output) return;
  const source = join(reportsDirectory, "lcov.info");
  const text = readFileSync(source, "utf8");
  const records = text.split("end_of_record");
  if (records.pop().trim() || !records.length) {
    throw new Error(`Incomplete Vitest coverage report: ${source}`);
  }
  const normalized = records
    .map((record) => {
      const lines = record.trim().split("\n");
      const sources = lines.filter((line) => line.startsWith("SF:"));
      if (sources.length !== 1) {
        throw new Error(`Invalid Vitest source record: ${source}`);
      }
      const original = sources[0].slice(3);
      const name = original.startsWith("angular/")
        ? original
        : `angular/${original}`;
      if (
        !name.startsWith("angular/") ||
        posix.normalize(name) !== name ||
        name.includes("\\")
      ) {
        throw new Error(`Invalid Vitest source path: ${original}`);
      }
      const counts = new Map();
      for (const line of lines.filter((line) => line.startsWith("DA:"))) {
        const match = /^DA:([1-9][0-9]*),([0-9]+)(?:,[^,]+)?$/.exec(line);
        if (!match || counts.has(Number(match[1]))) {
          throw new Error(`Invalid Vitest line record: ${name}`);
        }
        counts.set(Number(match[1]), Number(match[2]));
      }
      const found = lines.filter((line) => line.startsWith("LF:"));
      const hit = lines.filter((line) => line.startsWith("LH:"));
      if (
        found.length !== 1 ||
        found[0] !== `LF:${counts.size}` ||
        hit.length !== 1 ||
        hit[0] !==
          `LH:${[...counts.values()].filter((count) => count > 0).length}`
      ) {
        throw new Error(`Incomplete Vitest line coverage: ${name}`);
      }
      const branches = new Map();
      for (const line of lines.filter((line) => line.startsWith("BRDA:"))) {
        const match = /^BRDA:([1-9][0-9]*,[0-9]+,[0-9]+),(-|[0-9]+)$/.exec(
          line,
        );
        if (!match || branches.has(match[1])) {
          throw new Error(`Invalid Vitest branch record: ${name}`);
        }
        branches.set(match[1], match[2] === "-" ? 0 : Number(match[2]));
      }
      const branchFound = lines.filter((line) => line.startsWith("BRF:"));
      const branchHit = lines.filter((line) => line.startsWith("BRH:"));
      if (
        (branches.size || branchFound.length || branchHit.length) &&
        (branchFound.length !== 1 ||
          branchFound[0] !== `BRF:${branches.size}` ||
          branchHit.length !== 1 ||
          branchHit[0] !==
            `BRH:${[...branches.values()].filter((count) => count > 0).length}`)
      ) {
        throw new Error(`Incomplete Vitest branch coverage: ${name}`);
      }
      return (
        lines
          .map((line) => (line.startsWith("SF:") ? `SF:${name}` : line))
          .join("\n") + "\nend_of_record\n"
      );
    })
    .join("");
  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(output, normalized);
}
