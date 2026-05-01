import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export async function teardown() {
  const coverageOutputFile = process.env['COVERAGE_OUTPUT_FILE'];
  if (!coverageOutputFile) return;

  const __dirname = dirname(fileURLToPath(import.meta.url));
  const lcovSrc = join(
    process.env['TEST_TMPDIR'] ?? __dirname,
    '.vitest-coverage',
    'lcov.info',
  );

  if (!existsSync(lcovSrc)) {
    process.stderr.write(
      `[vitest-coverage] lcov file not found at: ${lcovSrc}\n`,
    );
    return;
  }

  const content = readFileSync(lcovSrc, 'utf-8');
  const fixed = content.replace(/^(SF:)(?!angular\/)/gm, '$1angular/');

  mkdirSync(dirname(coverageOutputFile), { recursive: true });
  writeFileSync(coverageOutputFile, fixed, 'utf-8');
}
