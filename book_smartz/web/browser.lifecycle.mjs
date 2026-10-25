/* global process */
import { appendFileSync, mkdirSync, writeSync } from "node:fs";
import { join } from "node:path";
import { performance } from "node:perf_hooks";
import { clearTimeout, setTimeout } from "node:timers";

export const recordPrefix = "browser-lifecycle ";
const phases = new Set([
  "assertions",
  "control_shutdown",
  "browser_shutdown",
  "proxy_shutdown",
  "journey_teardown",
  "fixture_exit",
  "collection_complete",
  "runner",
  "fixture_cleanup",
  "harness_complete",
  "child_exit",
]);

export function createLifecycle(role) {
  const started = performance.now();
  let sequence = 0;
  return {
    record(phase, state, status = {}) {
      if (!phases.has(phase)) return;
      const resource_types = process.getActiveResourcesInfo().sort();
      const record = {
        role,
        pid: process.pid,
        sequence: ++sequence,
        elapsed_ms: Math.floor(performance.now() - started),
        phase,
        state,
        resource_count: resource_types.length,
        resource_types,
      };
      for (const key of ["child_pid", "exit_code", "signal"])
        if (status[key] !== undefined) record[key] = status[key];
      if (phases.has(status.child_phase))
        record.child_phase = status.child_phase;
      const line = JSON.stringify(record) + "\n";
      try {
        writeSync(2, recordPrefix + line);
      } catch {
        /* Best effort. */
      }
      try {
        const directory = process.env.TEST_UNDECLARED_OUTPUTS_DIR;
        if (directory) {
          mkdirSync(directory, { recursive: true });
          appendFileSync(
            join(directory, `browser-lifecycle-${process.pid}.jsonl`),
            line,
          );
        }
      } catch {
        /* Diagnostic failure must preserve cleanup and test results. */
      }
    },
  };
}

// Each deadline bounds an actual cleanup operation. Every operation still runs.
export async function teardown(lifecycle, operations, timeoutMs = 10000) {
  let failure;
  for (const [phase, operation] of [
    ["control_shutdown", operations.controls],
    ["browser_shutdown", operations.browser],
    ["proxy_shutdown", operations.proxy],
  ]) {
    lifecycle.record(phase, "start");
    let timer;
    try {
      await Promise.race([
        Promise.resolve().then(operation),
        new Promise((_, reject) => {
          timer = setTimeout(
            () => reject(new Error(`${phase} exceeded its cleanup deadline`)),
            timeoutMs,
          );
        }),
      ]);
      lifecycle.record(phase, "complete");
    } catch (error) {
      lifecycle.record(phase, "failed");
      failure ??= error;
    } finally {
      clearTimeout(timer);
    }
  }
  if (failure) throw failure;
  lifecycle.record("journey_teardown", "complete");
}

// The fixture runs in its own process group. Timeout cleanup includes its children.
export function monitorChild(child, lifecycle, timeoutMs) {
  return new Promise((resolve, reject) => {
    let timedOut = false;
    let settled = false;
    let childPhase;
    let failedPhase;
    let pending = "";
    let forcedStop;
    const status = () => ({
      child_pid: child.pid,
      exit_code: child.exitCode,
      signal: child.signalCode,
      child_phase: childPhase ?? failedPhase,
    });
    const signal = (name) => {
      try {
        process.kill(-child.pid, name);
      } catch {
        /* The child can already be gone. */
      }
    };
    lifecycle.record("fixture_exit", "start", status());
    const deadline = setTimeout(() => {
      timedOut = true;
      lifecycle.record("fixture_exit", "failed", status());
      signal("SIGTERM");
      forcedStop = setTimeout(() => signal("SIGKILL"), 1000);
    }, timeoutMs);
    const finish = (error) => {
      if (settled) return;
      settled = true;
      clearTimeout(deadline);
      clearTimeout(forcedStop);
      lifecycle.record("fixture_exit", error ? "failed" : "complete", status());
      if (error) reject(error);
      else resolve();
    };
    child.stderr.on("data", (chunk) => {
      try {
        writeSync(2, chunk);
      } catch {
        /* Best effort. */
      }
      pending += chunk.toString();
      let newline;
      while ((newline = pending.indexOf("\n")) !== -1) {
        const line = pending.slice(0, newline);
        pending = pending.slice(newline + 1);
        if (!line.startsWith(recordPrefix)) continue;
        try {
          const record = JSON.parse(line.slice(recordPrefix.length));
          if (!phases.has(record.phase)) continue;
          if (record.state === "start") childPhase = record.phase;
          else if (record.state === "failed") {
            failedPhase = phases.has(record.child_phase)
              ? record.child_phase
              : record.phase;
            childPhase = undefined;
          } else if (record.state === "complete" && childPhase === record.phase)
            childPhase = undefined;
        } catch {
          /* Other stderr output is not a lifecycle record. */
        }
      }
    });
    child.stderr.on("error", () => {
      /* Diagnostic stream failure does not change the child result. */
    });
    child.once("error", finish);
    child.once("close", (code, signalName) => {
      const error = timedOut
        ? new Error("fixture exceeded its exit deadline")
        : code !== 0 || signalName !== null
          ? new Error("packaged browser fixture failed")
          : undefined;
      finish(error);
    });
  });
}
