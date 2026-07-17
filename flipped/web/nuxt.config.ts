import { fileURLToPath } from "node:url";
import type { NuxtConfig } from "nuxt/schema";
import tailwindcss from "@tailwindcss/vite";

const externalServerDependency = (id: string): boolean =>
  [
    "@grpc/",
    "@opentelemetry/",
    "@protobufjs/",
    "engine.io",
    "socket.io",
    "protobufjs",
    "long",
    "ws",
  ].some(
    (name) =>
      id === name ||
      id.startsWith(name) ||
      id.includes(`/node_modules/${name}`),
  );

export default {
  compatibilityDate: "2026-07-01",
  modules: ["@pinia/nuxt"],
  css: ["~/assets/css/main.css"],
  devtools: { enabled: false },
  vite: { plugins: [tailwindcss()] },
  nitro: {
    preset: "node-server",
    // @ts-expect-error Nitro's WebSocket feature is intentionally experimental.
    features: { websocket: true },
    // Bazel's pnpm runfiles are symlink forests; bundle server dependencies so
    // Nitro does not copy recursive package-manager links into the OCI layer.
    alias: {
      debug: fileURLToPath(
        new URL("./server/utils/debug-stub.ts", import.meta.url),
      ),
      events: fileURLToPath(
        new URL("./server/utils/node-events-shim.ts", import.meta.url),
      ),
      "vue-devtools-stub": fileURLToPath(
        new URL("./server/utils/vue-devtools-stub.ts", import.meta.url),
      ),
    },
    externals: {
      inline: [(id) => !externalServerDependency(id)],
      external: [externalServerDependency],
    },
  },
  app: {
    head: {
      title: "Flipped Examination",
      meta: [
        {
          name: "description",
          content: "Live examiner-led flashcard sessions",
        },
        { name: "referrer", content: "no-referrer" },
      ],
    },
  },
  routeRules: {
    "/**": {
      headers: {
        "referrer-policy": "no-referrer",
        "x-content-type-options": "nosniff",
        "x-frame-options": "DENY",
      },
    },
  },
  runtimeConfig: {
    grpcEndpoint: "127.0.0.1:50051",
    grpcProtoPath: "../proto/v1/examination.proto",
    oauthIssuer: "http://127.0.0.1:8080",
    oauthAudience: "flipped-session",
    oauthClientId: "",
    oauthClientSecret: "",
    cookieSecure: true,
    maxUploadBytes: 20_971_520,
    maxGlobalSockets: 4_096,
    maxSocketsPerSession: 8,
    maxPendingSocketJoins: 128,
    appOrigin: "http://127.0.0.1:3000",
    environment: "development",
    instanceId: "local",
    serviceVersion: "development",
    observabilityHmacKey: "",
    otlpEndpoint: "",
    public: {
      appName: "Flipped",
    },
  },
  typescript: {
    strict: true,
    typeCheck: false,
  },
} satisfies NuxtConfig;
