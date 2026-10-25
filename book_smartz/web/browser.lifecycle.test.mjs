/* global process */
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import {
  mkdtempSync,
  openSync,
  closeSync,
  readFileSync,
  readdirSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { URL } from "node:url";
import { once } from "node:events";
import { createLifecycle, monitorChild } from "./browser.lifecycle.mjs";

const lifecycleModule = new URL("./browser.lifecycle.mjs", import.meta.url)
  .href;
function run(mode, brokenDelivery = false) {
  const directory = mkdtempSync(join(tmpdir(), "browser-lifecycle-"));
  const full = brokenDelivery ? openSync("/dev/full", "w") : undefined;
  try {
    const program = `
      import { createLifecycle, teardown, runJourney } from ${JSON.stringify(lifecycleModule)};
      import { createServer } from "node:net";
      const lifecycle = createLifecycle("journey");
      const server = createServer();
      await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
      console.log("fixture:native-reader");
      const mode = ${JSON.stringify(mode)};
      const workFailure = ["assertion-failure", "combined-failure"].includes(mode);
      const journeyMode = workFailure || mode === "cleanup-journey-failure";
      const originalError = new Error("controlled assertion failure");
      const cleanupError = new Error("controlled cleanup failure");
      const operations = {
        controls() {},
        async browser() {
          if (mode === "browser-falsy-failure") throw undefined;
          if (["browser-failure", "combined-failure", "cleanup-journey-failure"].includes(mode)) throw cleanupError;
        },
        async proxy() {
          await new Promise(resolve => server.close(resolve));
          if (mode === "stall") await new Promise(() => {});
        },
      };
      try {
        if (journeyMode) {
          await runJourney(lifecycle, async () => {
            lifecycle.record("assertions", "start");
            if (workFailure) throw originalError;
            lifecycle.record("assertions", "complete");
          }, operations, 25);
        } else {
          lifecycle.record("assertions", "complete");
          await teardown(lifecycle, operations, 25);
        }
      } catch (error) {
        const expected = mode === "cleanup-journey-failure" ? cleanupError : originalError;
        process.exitCode = journeyMode && error !== expected ? 2 : 1;
      }
    `;
    const child = spawnSync(
      process.execPath,
      ["--input-type=module", "-e", program],
      {
        encoding: "utf8",
        timeout: 5000,
        env: {
          ...process.env,
          TEST_UNDECLARED_OUTPUTS_DIR: brokenDelivery ? "/dev/null" : directory,
        },
        stdio: ["ignore", "pipe", brokenDelivery ? full : "pipe"],
      },
    );
    assert.ifError(child.error);
    const records = readdirSync(directory).flatMap((name) =>
      readFileSync(join(directory, name), "utf8")
        .trim()
        .split("\n")
        .filter(Boolean)
        .map((line) => JSON.parse(line)),
    );
    return { child, records };
  } finally {
    if (full !== undefined) closeSync(full);
    rmSync(directory, { recursive: true, force: true });
  }
}

test("successful cleanup records ordered phases before completion", () => {
  const { child, records } = run("success");
  assert.equal(child.status, 0, child.stderr);
  assert.equal(child.stdout.trim(), "fixture:native-reader");
  assert.deepEqual(
    records.map(({ phase, state }) => [phase, state]),
    [
      ["assertions", "complete"],
      ["control_shutdown", "start"],
      ["control_shutdown", "complete"],
      ["browser_shutdown", "start"],
      ["browser_shutdown", "complete"],
      ["proxy_shutdown", "start"],
      ["proxy_shutdown", "complete"],
      ["journey_teardown", "complete"],
    ],
  );
  for (let index = 0; index < records.length; index++) {
    assert.equal(records[index].sequence, index + 1);
    assert.ok(
      records[index].elapsed_ms >= (records[index - 1]?.elapsed_ms ?? 0),
    );
    assert.deepEqual(Object.keys(records[index]).sort(), [
      "elapsed_ms",
      "phase",
      "pid",
      "resource_count",
      "resource_types",
      "role",
      "sequence",
      "state",
    ]);
  }
});

test("a stalled cleanup retains the incomplete phase and fails", () => {
  const { child, records } = run("stall");
  assert.equal(child.status, 1);
  assert.ok(
    records.some(
      (record) =>
        record.phase === "browser_shutdown" && record.state === "complete",
    ),
  );
  const failed = records.at(-1);
  assert.equal(failed.phase, "proxy_shutdown");
  assert.equal(failed.state, "failed");
  assert.ok(Array.isArray(failed.resource_types));
  assert.equal(
    records.some((record) => record.phase === "journey_teardown"),
    false,
  );
});

for (const mode of ["browser-failure", "browser-falsy-failure"])
  test(`a ${mode} still closes the proxy`, () => {
    const { child, records } = run(mode);
    assert.equal(child.status, 1);
    assert.ok(
      records.some(
        (record) =>
          record.phase === "browser_shutdown" && record.state === "failed",
      ),
    );
    assert.ok(
      records.some(
        (record) =>
          record.phase === "proxy_shutdown" && record.state === "complete",
      ),
    );
    assert.equal(
      records.some((record) => record.phase === "journey_teardown"),
      false,
    );
  });

for (const mode of [
  "assertion-failure",
  "combined-failure",
  "cleanup-journey-failure",
]) {
  test(`journey ${mode} preserves its error and completes cleanup`, () => {
    const { child, records } = run(mode);
    assert.equal(child.status, 1, child.stderr);
    if (mode !== "cleanup-journey-failure")
      assert.ok(
        records.some(
          (record) =>
            record.phase === "assertions" && record.state === "failed",
        ),
      );
    assert.ok(
      records.some(
        (record) =>
          record.phase === "proxy_shutdown" && record.state === "complete",
      ),
    );
    assert.equal(
      records.some((record) => record.phase === "journey_teardown"),
      mode === "assertion-failure",
    );
  });
}

for (const mode of [
  "success",
  "stall",
  "browser-failure",
  "assertion-failure",
  "combined-failure",
]) {
  test(`diagnostic delivery failure preserves ${mode} and cleanup`, () => {
    const { child } = run(mode, true);
    assert.equal(child.status, mode === "success" ? 0 : 1);
    assert.equal(child.stdout.trim(), "fixture:native-reader");
  });
}

for (const mode of [
  "success",
  "failure",
  "cleanup-failure",
  "stall",
  "ignore-term",
]) {
  test(`child supervision retains ${mode} status`, async () => {
    const child = spawn(
      process.execPath,
      [
        "--input-type=module",
        "-e",
        `
      import { createServer } from "node:net";
      const server = createServer();
      await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
      process.stderr.write('browser-lifecycle {"phase":"proxy_shutdown","state":"start"}\\n');
      if (${JSON.stringify(mode)} === "ignore-term") process.on("SIGTERM", () => {});
      process.on("message", () => {
        if (["stall", "ignore-term"].includes(${JSON.stringify(mode)})) return;
        if (${JSON.stringify(mode)} === "cleanup-failure") {
          for (const record of [
            { phase: "browser_shutdown", state: "failed" },
            { phase: "proxy_shutdown", state: "start" },
            { phase: "proxy_shutdown", state: "complete" },
          ]) process.stderr.write("browser-lifecycle " + JSON.stringify(record) + "\\n");
        }
        server.close(() => {
          process.exitCode = ${["failure", "cleanup-failure"].includes(mode) ? 7 : 0};
          process.disconnect();
        });
      });
      process.send("ready");
    `,
      ],
      { stdio: ["ignore", "pipe", "pipe", "ipc"], detached: true },
    );
    try {
      await once(child, "message");
      const result = monitorChild(
        child,
        createLifecycle("collection"),
        ["success", "failure", "cleanup-failure"].includes(mode) ? 2000 : 100,
      );
      child.send("finish");
      if (mode === "success") await result;
      else
        await assert.rejects(
          result,
          ["stall", "ignore-term"].includes(mode)
            ? /deadline/
            : /fixture failed/,
        );
      assert.notEqual(child.exitCode ?? child.signalCode, null);
      const records = readFileSync(
        join(
          process.env.TEST_UNDECLARED_OUTPUTS_DIR,
          `browser-lifecycle-${process.pid}.jsonl`,
        ),
        "utf8",
      )
        .trim()
        .split("\n")
        .map((line) => JSON.parse(line))
        .filter((record) => record.child_pid === child.pid);
      assert.equal(records[0].phase, "fixture_exit");
      assert.equal(
        records.at(-1).state,
        mode === "success" ? "complete" : "failed",
      );
      if (mode === "cleanup-failure")
        assert.equal(records.at(-1).child_phase, "browser_shutdown");
      if (["stall", "ignore-term"].includes(mode))
        assert.ok(
          records.some((record) => record.child_phase === "proxy_shutdown"),
        );
    } finally {
      if (child.exitCode === null && child.signalCode === null) {
        child.kill("SIGKILL");
        await once(child, "close");
      }
    }
  });
}

test("a failed child launch reports failure", async () => {
  const child = spawn("/nonexistent-browser-fixture", [], {
    stdio: ["ignore", "ignore", "pipe"],
  });
  await assert.rejects(
    monitorChild(child, createLifecycle("collection"), 100),
    /ENOENT/,
  );
});
