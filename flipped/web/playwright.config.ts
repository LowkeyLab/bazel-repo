import { join } from "node:path";
import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 180_000,
  globalTimeout: 600_000,
  reporter: "line",
  outputDir: join(process.env.TEST_UNDECLARED_OUTPUTS_DIR ?? ".", "playwright"),
});
