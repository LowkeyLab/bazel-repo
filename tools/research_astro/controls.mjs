import {
  cpSync,
  rmSync,
  chmodSync,
  writeFileSync,
  readFileSync,
  mkdirSync,
  readdirSync,
} from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";
const [merge, server, browser, output] = process.argv
  .slice(2)
  .map((value) => resolve(value));
mkdirSync(output, { recursive: true });
const controls = [
  "missing-profile",
  "corrupt-profile",
  "missing-map",
  "missing-script",
  "missing-completion",
  "interrupted",
];
const outcomes = [];
for (const control of controls) {
  const dir = resolve(output, control),
    s = resolve(dir, "server"),
    b = resolve(dir, "browser");
  mkdirSync(dir, { recursive: true });
  cpSync(server, s, { recursive: true });
  cpSync(browser, b, { recursive: true });
  if (control === "missing-profile") rmSync(resolve(b, "astro-profile.json"));
  if (control === "corrupt-profile") {
    chmodSync(resolve(b, "astro-profile.json"), 0o600);
    writeFileSync(resolve(b, "astro-profile.json"), "{corrupt");
  }
  if (control === "missing-map")
    rmSync(
      resolve(
        b,
        readdirSync(b).find((name) => name.endsWith(".map")),
      ),
    );
  if (control === "missing-script") {
    const p = JSON.parse(
      readFileSync(resolve(b, "astro-profile.json"), "utf8"),
    );
    rmSync(resolve(b, p.result[0].textFile));
  }
  if (control === "missing-completion")
    rmSync(resolve(b, "astro-completion.json"));
  if (control === "interrupted") {
    cpSync(
      resolve(b, "interrupted-functional.json"),
      resolve(b, "astro-functional.json"),
    );
    rmSync(resolve(b, "astro-profile.json"));
    rmSync(resolve(b, "astro-completion.json"));
  }
  const result = spawnSync(merge, [s, b, resolve(dir, "converted")], {
    encoding: "utf8",
    maxBuffer: 4 * 1024 * 1024,
  });
  writeFileSync(resolve(dir, "collector.stderr"), result.stderr || "");
  const functionalPassed = JSON.parse(
    readFileSync(resolve(b, "astro-functional.json"), "utf8"),
  ).passed;
  outcomes.push({ control, exit: result.status, functionalPassed });
  if (result.status !== 1 || !functionalPassed)
    throw Error(`Unexpected negative result: ${control}`);
}
writeFileSync(
  resolve(output, "controls.json"),
  JSON.stringify(outcomes, null, 2),
);
console.log(JSON.stringify(outcomes));
