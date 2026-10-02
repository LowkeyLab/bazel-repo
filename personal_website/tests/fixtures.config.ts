import base from "../astro.config";
import { defineConfig } from "astro/config";

export default defineConfig({
  ...base,
  srcDir: "./tests/empty",
  outDir: "./tests/dist",
  integrations: [
    ...(base.integrations ?? []),
    {
      name: "browser-fixtures",
      hooks: {
        "astro:config:setup": ({ injectRoute }) => {
          for (const route of [
            "hero",
            "lifecycle",
            "featured-work",
            "independence",
            "recent-writing",
          ]) {
            injectRoute({
              pattern: `/tests/${route}`,
              entrypoint: `./tests/fixtures/${route}.astro`,
            });
          }
        },
      },
    },
  ],
});
