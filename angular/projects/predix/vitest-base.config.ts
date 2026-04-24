import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Write coverage to a known location relative to the config file so the
// globalSetup teardown can reliably find it.
const reportsDirectory = join(__dirname, '.vitest-coverage');

export default defineConfig({
  test: {
    coverage: {
      reportsDirectory,
    },
    // Always wire the global setup — teardown is a no-op when not under
    // `bazel coverage` (checks COVERAGE_OUTPUT_FILE at runtime).
    globalSetup: [join(__dirname, 'vitest-global-setup.ts')],
  },
});
