/* global process */
import { spawn } from "node:child_process";
import { resolve } from "node:path";
import { createLifecycle, monitorChild } from "./browser.lifecycle.mjs";

// Run the same packaged journey under the native Node collector.
// Node profiles then include the real journey executed by the Rust fixture.
const fixture = resolve(
  process.env.JS_BINARY__RUNFILES,
  process.env.JS_BINARY__WORKSPACE,
  process.env.BROWSER_FIXTURE,
);
const lifecycle = createLifecycle("collection");
const child = spawn(fixture, ["--nocapture"], {
  stdio: ["inherit", "inherit", "pipe"],
  detached: true,
});
// Retain a final diagnostic and allow termination before Bazel's outer timeout.
const budget = Math.max(
  1,
  Number(process.env.TEST_TIMEOUT || 900) * 1000 - 15000,
);
await monitorChild(child, lifecycle, budget);
lifecycle.record("collection_complete", "complete");
