import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const configDirectory = dirname(fileURLToPath(import.meta.url));
const coverageDirectory = join(
  process.env['TEST_TMPDIR'] ?? configDirectory,
  '.vitest-coverage',
);

export async function setup() {
  mkdirSync(coverageDirectory, { recursive: true });
}

export async function teardown() {
  const coverageOutputFile = process.env['COVERAGE_OUTPUT_FILE'];
  // No-op when not running under `bazel coverage`.
  if (!coverageOutputFile) return;

  const lcovSrc = join(coverageDirectory, 'lcov.info');

  if (!existsSync(lcovSrc)) {
    process.stderr.write(
      `[vitest-coverage] lcov file not found at: ${lcovSrc}\n`,
    );
    return;
  }

  // The Angular CLI's vitest runner sets root=workspaceRoot (angular/), so
  // coverage paths are relative to angular/ rather than the repo root.
  // Bazel's instrumentation_filter and lcov aggregator expect paths prefixed
  // with "angular/", so we rewrite SF: lines before emitting.
  const content = readFileSync(lcovSrc, 'utf-8');
  const fixed = content.replace(/^(SF:)(?!angular\/)/gm, '$1angular/');

  mkdirSync(dirname(coverageOutputFile), { recursive: true });
  writeFileSync(coverageOutputFile, fixed, 'utf-8');
  const sfCount = fixed.split('\n').filter((l) => l.startsWith('SF:')).length;
  process.stderr.write(
    `[vitest-coverage] Wrote ${sfCount} SF: entries to: ${coverageOutputFile}\n`,
  );
}
