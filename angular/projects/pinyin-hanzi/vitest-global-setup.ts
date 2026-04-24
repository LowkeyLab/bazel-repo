import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export async function teardown() {
  const coverageOutputFile = process.env.COVERAGE_OUTPUT_FILE;

  if (!coverageOutputFile) {
    return;
  }

  const __dirname = dirname(fileURLToPath(import.meta.url));
  const lcovSource = join(__dirname, '.vitest-coverage', 'lcov.info');

  if (!existsSync(lcovSource)) {
    process.stderr.write(
      `[vitest-coverage] lcov file not found at: ${lcovSource}\n`,
    );
    return;
  }

  const content = readFileSync(lcovSource, 'utf-8');
  const fixedContent = content.replace(/^(SF:)(?!angular\/)/gm, '$1angular/');

  mkdirSync(dirname(coverageOutputFile), { recursive: true });
  writeFileSync(coverageOutputFile, fixedContent, 'utf-8');

  const sourceFileCount = fixedContent
    .split('\n')
    .filter((line) => line.startsWith('SF:')).length;

  process.stderr.write(
    `[vitest-coverage] Wrote ${sourceFileCount} SF: entries to: ${coverageOutputFile}\n`,
  );
}
