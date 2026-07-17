import { createReadStream, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { generateKeyPairSync, randomBytes } from "node:crypto";
import { chromium, expect, test, type Browser } from "@playwright/test";
import Docker from "dockerode";
import {
  GenericContainer,
  Network,
  Wait,
  type StartedNetwork,
  type StartedTestContainer,
} from "testcontainers";

const workspace = join(
  process.env.TEST_SRCDIR ?? "",
  process.env.TEST_WORKSPACE ?? "_main",
);
const runfile = (path: string): string => join(workspace, path);

async function loadImage(docker: Docker, path: string): Promise<void> {
  const stream = await new Promise<NodeJS.ReadableStream>((resolve, reject) => {
    docker.loadImage(createReadStream(path), (error, response) => {
      if (error || !response)
        reject(error ?? new Error("missing Docker response"));
      else resolve(response);
    });
  });
  await new Promise<void>((resolve, reject) => {
    docker.modem.followProgress(stream, (error) => {
      if (error) reject(error);
      else resolve();
    });
  });
}

function secretsDirectory(): string {
  const directory = mkdtempSync(join(tmpdir(), "flipped-e2e-"));
  const { privateKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });
  writeFileSync(
    join(directory, "jwt-private.pem"),
    privateKey.export({ type: "pkcs8", format: "pem" }),
  );
  writeFileSync(join(directory, "oauth-client-secret"), "e2e-client-secret");
  writeFileSync(
    join(directory, "invitation-hmac"),
    randomBytes(32).toString("base64url"),
  );
  writeFileSync(
    join(directory, "observability-hmac"),
    randomBytes(32).toString("base64url"),
  );
  return directory;
}

let network: StartedNetwork | undefined;
let server: StartedTestContainer | undefined;
let web: StartedTestContainer | undefined;
let browserServer: StartedTestContainer | undefined;
let browser: Browser | undefined;

async function stop(
  container: StartedTestContainer | undefined,
): Promise<void> {
  await container?.stop().catch(() => undefined);
}

const capturedLogs = new Map<string, string>();

async function pipeLogs(
  name: string,
  container: StartedTestContainer,
): Promise<void> {
  const logs = await container.logs({ tail: 100 });
  logs.on("data", (chunk: Buffer) => {
    const text = chunk.toString("utf8");
    capturedLogs.set(name, `${capturedLogs.get(name) ?? ""}${text}`);
    process.stdout.write(`[${name}] ${text}`);
  });
}

test.beforeAll(async () => {
  const docker = new Docker();
  await Promise.all([
    loadImage(
      docker,
      runfile("flipped/server/bin/image_tarball_archive/tarball.tar"),
    ),
    loadImage(docker, runfile("flipped/web/image_tarball_archive/tarball.tar")),
  ]);
  network = await new Network().start();
  const secrets = secretsDirectory();
  server = await new GenericContainer("local/flipped-server:e2e")
    .withNetwork(network)
    .withNetworkAliases("flipped-server")
    .withExposedPorts(50051, 8080)
    .withBindMounts([{ source: secrets, target: "/run/secrets", mode: "ro" }])
    .withEnvironment({
      FLIPPED_GRPC_ADDR: "0.0.0.0:50051",
      FLIPPED_HTTP_ADDR: "0.0.0.0:8080",
      FLIPPED_OAUTH_ISSUER: "http://flipped-server:8080",
      FLIPPED_OAUTH_AUDIENCE: "flipped-session",
      FLIPPED_OAUTH_CLIENT_ID: "flipped-web",
      FLIPPED_OAUTH_CLIENT_SECRET_FILE: "/run/secrets/oauth-client-secret",
      FLIPPED_JWT_ACTIVE_PRIVATE_KEY_FILE: "/run/secrets/jwt-private.pem",
      FLIPPED_JWT_ACTIVE_KID: "e2e-key",
      FLIPPED_INVITATION_HMAC_KEY_FILE: "/run/secrets/invitation-hmac",
      FLIPPED_OBSERVABILITY_HMAC_KEY_FILE: "/run/secrets/observability-hmac",
      FLIPPED_ENVIRONMENT: "test",
      FLIPPED_INSTANCE_ID: "e2e-server",
      FLIPPED_SERVICE_VERSION: "e2e",
    })
    .withWaitStrategy(Wait.forLogMessage(/service\.ready/))
    .start();
  web = await new GenericContainer("local/flipped-web:e2e")
    .withNetwork(network)
    .withNetworkAliases("flipped-web")
    .withExposedPorts(3000)
    .withEnvironment({
      NUXT_GRPC_ENDPOINT: "flipped-server:50051",
      NUXT_OAUTH_ISSUER: "http://flipped-server:8080",
      NUXT_OAUTH_AUDIENCE: "flipped-session",
      NUXT_OAUTH_CLIENT_ID: "flipped-web",
      NUXT_OAUTH_CLIENT_SECRET: "e2e-client-secret",
      NUXT_COOKIE_SECURE: "false",
      NUXT_APP_ORIGIN: "http://flipped-web:3000",
      NUXT_ENVIRONMENT: "test",
      NUXT_INSTANCE_ID: "e2e-web",
      NUXT_SERVICE_VERSION: "e2e",
      NUXT_OBSERVABILITY_HMAC_KEY: randomBytes(32).toString("base64url"),
    })
    .withWaitStrategy(Wait.forHttp("/api/health", 3000).forStatusCode(200))
    .start();
  await pipeLogs("server", server);
  await pipeLogs("web", web);
  browserServer = await new GenericContainer(
    "mcr.microsoft.com/playwright:v1.61.1-noble",
  )
    .withNetwork(network)
    .withCommand([
      "/bin/sh",
      "-c",
      "npx playwright run-server --host 0.0.0.0 --port 3000",
    ])
    .withExposedPorts(3000)
    .withWaitStrategy(Wait.forListeningPorts())
    .start();
  browser = await chromium.connect(
    `ws://${browserServer.getHost()}:${browserServer.getMappedPort(3000)}/`,
  );
});

test.afterAll(async () => {
  await browser?.close().catch(() => undefined);
  await stop(browserServer);
  await stop(web);
  await stop(server);
  await network?.stop().catch(() => undefined);
});

test("runs the complete examiner-led two-card session without leaking answers", async () => {
  if (!browser) throw new Error("browser did not start");
  const origin = "http://flipped-web:3000";
  const testTakerContext = await browser.newContext();
  await testTakerContext.addInitScript(() => {
    Object.defineProperty(navigator, "clipboard", {
      value: {
        writeText: async (text: string) => {
          window.sessionStorage.setItem("examiner-invitation", text);
        },
      },
    });
  });
  const examinerContext = await browser.newContext();
  const testTaker = await testTakerContext.newPage();
  const examiner = await examinerContext.newPage();

  await testTaker.goto(origin);
  await testTaker
    .locator('input[type="file"]')
    .setInputFiles(runfile("flipped/e2e/fixture/ordinary.apkg"));
  const invitation = testTaker.getByText("Invite the examiner");
  const uploadError = testTaker.getByRole("alert");
  await Promise.race([
    invitation.waitFor({ state: "visible" }),
    uploadError.waitFor({ state: "visible" }),
  ]);
  if (await uploadError.isVisible()) {
    const message = await uploadError.textContent();
    process.stdout.write(
      `[server-current] ${capturedLogs.get("server") ?? ""}\n`,
    );
    process.stdout.write(`[web-current] ${capturedLogs.get("web") ?? ""}\n`);
    throw new Error(`upload failed: ${message}`);
  }
  await expect(invitation).toBeVisible();
  await testTaker.getByRole("button", { name: "Copy examiner link" }).click();
  const invitationUrl = await testTaker.evaluate(() =>
    window.sessionStorage.getItem("examiner-invitation"),
  );
  if (!invitationUrl) throw new Error("invitation was not copied");
  await testTaker
    .getByRole("link", { name: "Enter test-taker session" })
    .click();
  await expect(
    testTaker.getByText("Test taker", { exact: true }),
  ).toBeVisible();

  await examiner.goto(invitationUrl);
  await expect(examiner.getByText("Examiner", { exact: true })).toBeVisible();
  await expect(examiner.getByText("Test taker connected")).toBeVisible();
  await examiner.getByRole("button", { name: "Start examination" }).click();

  await expect(examiner.getByText("question 1")).toBeVisible();
  await expect(examiner.getByText("answer 1")).toBeVisible();
  await expect(testTaker.getByText("question 1")).toBeVisible();
  await expect(testTaker.getByText("answer 1")).toHaveCount(0);

  await examiner.getByRole("button", { name: "Next card" }).click();
  await expect(examiner.getByText("question 2")).toBeVisible();
  await expect(testTaker.getByText("question 2")).toBeVisible();
  await expect(testTaker.getByText("answer 2")).toHaveCount(0);

  await examiner.getByRole("button", { name: "Next card" }).click();
  await expect(
    examiner.getByRole("heading", { name: "completed" }),
  ).toBeVisible();
  await expect(
    testTaker.getByRole("heading", { name: "completed" }),
  ).toBeVisible();
  await examiner.getByRole("button", { name: "End session" }).click();
  await examiner
    .getByRole("dialog")
    .getByRole("button", { name: "End session" })
    .click();
  await expect(
    examiner.getByRole("heading", { name: "terminated" }),
  ).toBeVisible();
  await expect(
    testTaker.getByRole("heading", { name: "terminated" }),
  ).toBeVisible();

  await examinerContext.close();
  await testTakerContext.close();
});
