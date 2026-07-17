import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

const coverageDirectory = join(
  process.env.TEST_TMPDIR ?? process.cwd(),
  ".vitest-coverage",
);

export function setup(): void {
  mkdirSync(coverageDirectory, { recursive: true });
}

export function teardown(): void {
  const output = process.env.COVERAGE_OUTPUT_FILE;
  if (!output) return;
  const source = join(coverageDirectory, "lcov.info");
  if (!existsSync(source))
    throw new Error("Vitest LCOV output was not produced");
  const normalized = readFileSync(source, "utf8").replace(
    /^(SF:)(?!flipped\/web\/)/gm,
    "$1flipped/web/",
  );
  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(output, normalized, "utf8");
}
