import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

const __dirname = dirname(fileURLToPath(import.meta.url));
const reportsDirectory = join(__dirname, '.vitest-coverage');

export default defineConfig({
  test: {
    coverage: {
      reportsDirectory,
    },
    globalSetup: [join(__dirname, 'vitest-global-setup.ts')],
  },
});
