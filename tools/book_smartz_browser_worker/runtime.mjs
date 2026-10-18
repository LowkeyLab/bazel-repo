import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import console from "node:console";
import process from "node:process";
import { readFileSync } from "node:fs";
import { setTimeout } from "node:timers/promises";
import { chromium } from "playwright";

function docker(...args) {
  const result = spawnSync("docker", args, {
    encoding: "utf8",
    timeout: 120_000,
  });
  assert.equal(
    result.status,
    0,
    `${args.join(" ")}: ${result.error ?? result.stderr}`,
  );
  return result.stdout.trim();
}

async function waitUntil(description, check) {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    if (check()) return;
    await setTimeout(1000);
  }
  throw new Error(`Timed out waiting for ${description}`);
}

assert.equal(process.env.CHROMIUM_EXECUTABLE, undefined);
assert.equal(process.env.CHROME_BIN, undefined);
assert.ok(chromium.executablePath().startsWith("/ms-playwright/"));
const daemon = spawn("dockerd", ["--storage-driver=vfs"], { stdio: "inherit" });
daemon.on("error", (error) => {
  throw error;
});
let browser;
try {
  await waitUntil("Docker daemon", () => {
    assert.equal(daemon.exitCode, null, "Docker daemon exited during startup");
    return (
      spawnSync("docker", ["info"], { stdio: "ignore", timeout: 5000 })
        .status === 0
    );
  });
  console.log(`Docker ${docker("version", "--format", "{{.Server.Version}}")}`);
  docker("load", "--input", "/fixtures/postgres.tar");
  const image = readFileSync("/fixtures/postgres.ref.txt", "utf8").trim();
  const container = docker(
    "run",
    "--detach",
    "--pull=never",
    "--name",
    "worker-postgres",
    "--env",
    "POSTGRES_PASSWORD=worker-runtime-probe",
    image,
  );
  try {
    await waitUntil(
      "PostgreSQL",
      () =>
        spawnSync(
          "docker",
          ["exec", container, "pg_isready", "--username=postgres"],
          { stdio: "ignore", timeout: 5000 },
        ).status === 0,
    );
    assert.equal(
      docker(
        "exec",
        container,
        "psql",
        "--username=postgres",
        "--tuples-only",
        "--no-align",
        "--command",
        "SELECT 6 * 7",
      ),
      "42",
    );
    browser = await chromium.launch({ headless: true });
    const page = await browser.newPage();
    await page.setContent(
      "<title>Worker runtime ready</title><button>Chromium and PostgreSQL</button>",
    );
    assert.equal(await page.title(), "Worker runtime ready");
    assert.equal(
      await page.getByRole("button").textContent(),
      "Chromium and PostgreSQL",
    );
    console.log(
      `Chromium ${browser.version()} at ${chromium.executablePath()}`,
    );
    console.log(`PostgreSQL ${image}: SELECT 6 * 7 = 42; browser launch PASS`);
  } finally {
    if (browser) await browser.close();
    docker("rm", "--force", "--volumes", container);
  }
} finally {
  daemon.kill("SIGTERM");
}
