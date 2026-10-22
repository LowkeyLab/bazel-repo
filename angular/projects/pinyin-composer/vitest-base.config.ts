import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';
import { coverage } from '../../bzl/vitest-coverage.mjs';

const configDirectory = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  test: {
    coverage,
    globalSetup: [join(configDirectory, 'vitest-global-setup.ts')],
  },
});
