const postcss = require("postcss");
const { readFileSync, writeFileSync, mkdirSync } = require("fs");
const { resolve, dirname } = require("path");

const args = process.argv.slice(2);
const src = args[0];
const dest = args[1];
const configFile = args[2];

if (!src || !dest) {
  console.error("Usage: node process_css.js <src> <dest> [config]");
  process.exit(1);
}

(async () => {
  try {
    const css = readFileSync(src, "utf8");

    mkdirSync(dirname(dest), { recursive: true });

    let plugins = [];
    if (configFile) {
      const configPath = resolve(configFile);
      const config = require(configPath);
      if (config.plugins) {
        for (const [name, options] of Object.entries(config.plugins)) {
          plugins.push(require(name)(options));
        }
      }
    }

    const result = await postcss(plugins).process(css, { from: src, to: dest });

    writeFileSync(dest, result.css);
    if (result.map) {
      writeFileSync(dest + ".map", result.map.toString());
    }
  } catch (e) {
    console.error("PostCSS processing failed:", e);
    process.exit(1);
  }
})();
