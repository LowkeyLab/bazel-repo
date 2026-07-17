import vue from "eslint-plugin-vue";
import root from "../../eslint.config.mjs";

const exporterModules = [
  "pino",
  "@opentelemetry/api",
  "@opentelemetry/sdk-node",
  "@opentelemetry/sdk-metrics",
  "@opentelemetry/resources",
  "@opentelemetry/exporter-trace-otlp-http",
  "@opentelemetry/exporter-metrics-otlp-http",
];

export default [
  ...root,
  ...vue.configs["flat/recommended"],
  {
    files: ["**/*.{ts,vue}"],
    rules: {
      "no-console": "error",
      "no-restricted-imports": [
        "error",
        {
          paths: exporterModules.map((name) => ({
            name,
            message: "Emit a typed EventListener event instead.",
          })),
        },
      ],
    },
  },
  {
    files: ["server/utils/observability.ts"],
    rules: { "no-restricted-imports": "off" },
  },
];
