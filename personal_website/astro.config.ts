import { createHash } from "node:crypto";
import { glob, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { defineConfig } from "astro/config";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  site: "https://www.tacascer.com",
  trailingSlash: "always",
  integrations: [
    {
      name: "content-etags",
      hooks: {
        "astro:build:done": async ({ dir }) => {
          // Caddy's timestamp/size validators are unreliable after pkg_tar
          // normalizes timestamps. Stable URLs need content-based validators.
          for await (const entry of glob("**/*", {
            cwd: dir,
            withFileTypes: true,
            exclude: ["_astro/**", "**/*.etag"],
          })) {
            if (!entry.isFile()) continue;
            const path = join(entry.parentPath, entry.name);
            const hash = createHash("sha256")
              .update(await readFile(path))
              .digest("hex");
            await writeFile(`${path}.etag`, `"${hash}"`);
          }
        },
      },
    },
  ],
  vite: {
    // Bazel also supplies emitted JavaScript. Coverage builds select maintained
    // TypeScript inputs so Vite can produce their original source maps.
    resolve:
      process.env.BROWSER_COVERAGE === "1"
        ? {
            extensions: [".mts", ".ts", ".mjs", ".js", ".jsx", ".tsx", ".json"],
          }
        : undefined,
    build: { sourcemap: process.env.BROWSER_COVERAGE === "1" },
    plugins: [tailwindcss()],
    server: {
      fs: {
        allow: [".."],
      },
    },
  },
});
