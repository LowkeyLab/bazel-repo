import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Write coverage to a writable Bazel test temp directory when available so
// Vitest does not try to clean up files under the read-only runfiles tree.
const reportsDirectory = join(
  process.env.TEST_TMPDIR ?? __dirname,
  '.vitest-coverage',
);

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
