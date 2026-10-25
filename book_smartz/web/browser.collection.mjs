/* global process */
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { resolve } from "node:path";

// Run the same packaged journey under the native Node collector.
// Node profiles then include the real journey executed by the Rust fixture.
const fixture = resolve(
  process.env.JS_BINARY__RUNFILES,
  process.env.JS_BINARY__WORKSPACE,
  process.env.BROWSER_FIXTURE,
);
const child = spawn(fixture, ["--nocapture"], { stdio: "inherit" });
const [status] = await once(child, "exit");
assert.equal(status, 0, "The packaged browser fixture must pass.");
