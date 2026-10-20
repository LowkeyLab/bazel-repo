import { defineConfig } from "astro/config";
export default defineConfig({
  vite: { build: { sourcemap: true } },
  devToolbar: { enabled: false },
});
