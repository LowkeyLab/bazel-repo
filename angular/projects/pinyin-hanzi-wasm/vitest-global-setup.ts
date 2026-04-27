import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';

const coverageOutputFile = process.env['COVERAGE_OUTPUT_FILE'];
const coverageInputDirectory = join(
  process.env['TEST_TMPDIR'] ?? __dirname,
  'vitest-coverage',
);

export default function setup(): () => void {
  return () => {
    if (!coverageOutputFile) {
      return;
    }

    const source = join(coverageInputDirectory, 'lcov.info');
    if (!existsSync(source)) {
      return;
    }

    mkdirSync(dirname(coverageOutputFile), { recursive: true });
    copyFileSync(source, coverageOutputFile);
  };
}
