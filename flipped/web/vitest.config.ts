import { join } from "node:path";
import { fileURLToPath } from "node:url";
import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vitest/config";

const reportsDirectory = join(
  process.env.TEST_TMPDIR ?? process.cwd(),
  ".vitest-coverage",
);

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      "#shared": fileURLToPath(new URL("./shared", import.meta.url)),
      "~": fileURLToPath(new URL("./app", import.meta.url)),
      "@": fileURLToPath(new URL("./app", import.meta.url)),
    },
  },
  test: {
    include: ["tests/unit/**/*.spec.ts"],
    globalSetup: ["./vitest-global-setup.ts"],
    coverage: {
      enabled: Boolean(process.env.COVERAGE_OUTPUT_FILE),
      provider: "v8",
      reporter: ["text", "lcov"],
      reportsDirectory,
      include: ["app/**/*.{ts,vue}", "server/**/*.ts", "shared/**/*.ts"],
      exclude: [
        "app/app.vue",
        "server/plugins/**",
        "server/middleware/**",
        "server/utils/debug-stub.ts",
        "server/utils/node-events-shim.ts",
        "server/utils/vue-devtools-stub.ts",
        "**/*.d.ts",
      ],
    },
  },
});
