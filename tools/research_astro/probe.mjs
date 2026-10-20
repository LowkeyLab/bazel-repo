import { createRequire } from "node:module";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
const require = createRequire(import.meta.url);
const astroRequire = createRequire(require.resolve("astro/package.json"));
const compilerPath = astroRequire.resolve("@astrojs/compiler-rs");
const compiler = await import(compilerPath);
const [input, output] = process.argv.slice(2);
mkdirSync(output, { recursive: true });
const source = readFileSync(input, "utf8");
const result = await compiler.transform(source, {
  filename: input,
  sourcemap: "both",
});
writeFileSync(
  resolve(output, "transform.json"),
  JSON.stringify(result, null, 2),
);
writeFileSync(
  resolve(output, "parse.json"),
  JSON.stringify(await compiler.parse(source, { position: true }), null, 2),
);
writeFileSync(resolve(output, "compiled.mjs"), result.code);
writeFileSync(
  resolve(output, "compiler-source.txt"),
  readFileSync(compilerPath, "utf8"),
);
console.log(
  JSON.stringify(
    {
      compilerPath,
      compilerExports: Object.keys(compiler),
      transformKeys: Object.keys(result),
      scripts: result.scripts,
      map: result.map,
    },
    null,
    2,
  ),
);
const coveragePath = require.resolve("@vitest/coverage-v8", {
  paths: [resolve("angular/node_modules")],
});
const coverageRequire = createRequire(coveragePath);
writeFileSync(
  resolve(output, "coverage-v8.txt"),
  readFileSync(coveragePath, "utf8"),
);
for (const name of ["ast-v8-to-istanbul", "@vitest/istanbul-lib-coverage"]) {
  const entry = coverageRequire.resolve(name);
  console.log(name, entry);
  if (name === "ast-v8-to-istanbul") {
    const nested = createRequire(entry);
    for (const dep of ["@jridgewell/trace-mapping", "acorn"]) {
      try {
        const file = nested.resolve(dep);
        writeFileSync(
          resolve(output, dep.replaceAll("/", "_") + ".txt"),
          readFileSync(file, "utf8"),
        );
      } catch (error) {
        console.log(dep, error.code);
      }
    }
  }
  writeFileSync(
    resolve(output, name.replaceAll("/", "_") + ".txt"),
    readFileSync(entry, "utf8"),
  );
}
