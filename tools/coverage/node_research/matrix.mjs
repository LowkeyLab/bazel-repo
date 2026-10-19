import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
  symlinkSync,
  chmodSync,
  rmSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const root = process.cwd();
const here = dirname(fileURLToPath(import.meta.url));
const out = process.env.TEST_UNDECLARED_OUTPUTS_DIR;
const tmp = process.env.TEST_TMPDIR;
chmodSync(tmp, 0o755);
mkdirSync(out, { recursive: true });
const results = [];
function run(name, args, options = {}) {
  const r = spawnSync(process.execPath, args, {
    cwd: root,
    env: {
      ...process.env,
      NODE_V8_COVERAGE: "",
      COVERAGE_OUTPUT_FILE: "",
      ...options.env,
    },
    encoding: "utf8",
    timeout: 180000,
    ...options,
  });
  writeFileSync(join(out, `${name}.stdout`), r.stdout ?? "");
  writeFileSync(join(out, `${name}.stderr`), r.stderr ?? "");
  results.push({
    name,
    status: r.status,
    signal: r.signal,
    error: r.error?.message,
  });
  return r;
}
// Execute the exact maintained teardown functions against controlled reports.
for (const project of [
  "mindreadr",
  "nicknamer",
  "nicknamer2-web",
  "pinyin-composer",
  "predix",
  "tailwind-sample",
]) {
  for (const kind of ["absent", "empty", "partial"]) {
    const area = join(tmp, `${project}-${kind}`);
    const config = join(area, "runfiles");
    const writable = join(area, "tmp");
    mkdirSync(config, { recursive: true });
    mkdirSync(writable, { recursive: true });
    const source = readFileSync(
      join(here, "adapters", `${project}.input`),
      "utf8",
    );
    writeFileSync(join(config, "setup.mjs"), source);
    const local = ["predix", "tailwind-sample"].includes(project);
    const report = join(local ? config : writable, ".vitest-coverage");
    if (kind !== "absent") {
      mkdirSync(report, { recursive: true });
      writeFileSync(
        join(report, "lcov.info"),
        kind === "empty"
          ? ""
          : "SF:projects/research/src/production.ts\nDA:6,1\nLF:1\nLH:1\nend_of_record\n",
      );
    }
    const dest = join(writable, "coverage.dat");
    const r = run(
      `${project}-${kind}`,
      [
        "--input-type=module",
        "-e",
        `const m=await import(${JSON.stringify(join(config, "setup.mjs"))}); await m.setup?.(); await m.teardown();`,
      ],
      {
        env: {
          ...process.env,
          NODE_V8_COVERAGE: "",
          TEST_TMPDIR: writable,
          COVERAGE_OUTPUT_FILE: dest,
        },
      },
    );
    results.at(-1).outputExists = existsSync(dest);
    results.at(-1).outputBytes = existsSync(dest)
      ? readFileSync(dest).length
      : null;
    if (existsSync(dest)) cpSync(dest, join(out, `${project}-${kind}.lcov`));
    assert.equal(r.status, 0);
  }
}
// Check report-directory creation while the configuration tree is read-only.
for (const project of ["predix", "tailwind-sample", "nicknamer"]) {
  const area = join(tmp, `${project}-readonly`);
  mkdirSync(area, { recursive: true });
  const config = join(area, "runfiles");
  mkdirSync(config);
  chmodSync(config, 0o555);
  const writable = join(area, "tmp");
  mkdirSync(writable);
  chmodSync(writable, 0o777);
  const chosen = ["predix", "tailwind-sample"].includes(project)
    ? config
    : writable;
  const r = run(
    `${project}-readonly`,
    [
      "--input-type=module",
      "-e",
      `import {mkdirSync,writeFileSync} from 'node:fs'; import {join} from 'node:path'; const p=join(${JSON.stringify(chosen)},'.vitest-coverage');mkdirSync(p,{recursive:true});writeFileSync(join(p,'lcov.info'),'');`,
    ],
    { uid: process.getuid() === 0 ? 65534 : process.getuid() },
  );
  results.at(-1).readOnlyConfiguration = true;
  results.at(-1).effectiveUid =
    process.getuid() === 0 ? 65534 : process.getuid();
  chmodSync(config, 0o755);
}
// A minimal Angular CLI project uses the pinned builder and provider.
const workspace = join(tmp, "angular-workspace");
mkdirSync(join(workspace, "projects/research/src"), { recursive: true });
symlinkSync(
  join(root, "angular/node_modules"),
  join(workspace, "node_modules"),
  "dir",
);
cpSync(join(here, "capture.mjs"), join(workspace, "capture.mjs"));
writeFileSync(
  join(workspace, "package.json"),
  JSON.stringify({ type: "module" }),
);
const src = join(workspace, "projects/research/src");
writeFileSync(join(src, "main.ts"), "export {};");
for (const f of [
  "production.ts",
  "helper.ts",
  "disconnected.ts",
  "child_only.ts",
])
  cpSync(join(here, f), join(src, f));
writeFileSync(
  join(src, "child.mjs"),
  `import { childOnly } from './child_only.ts'; if(childOnly()!==41) throw Error('child');`,
);
writeFileSync(
  join(src, "case.spec.ts"),
  `import {it,expect} from 'vitest'; import {spawnSync} from 'node:child_process'; import {choose} from './production';import {twice} from './helper';it('parent and awaited child',()=>{expect(choose(1)).toBe(2);expect(twice(3)).toBe(6);const c=spawnSync(process.execPath,[${JSON.stringify(join(src, "child.mjs"))}],{encoding:'utf8'});expect(c.status,c.stderr).toBe(0);});`,
);
writeFileSync(
  join(workspace, "tsconfig.json"),
  JSON.stringify({
    compilerOptions: {
      target: "ES2022",
      module: "preserve",
      moduleResolution: "bundler",
      skipLibCheck: true,
      types: ["node", "vitest/globals"],
    },
    include: ["projects/**/*.ts"],
  }),
);
for (const mode of ["defaults", "include", "autoattach"]) {
  const report = join(out, `angular-${mode}`);
  mkdirSync(report, { recursive: true });
  writeFileSync(
    join(workspace, "vitest.config.ts"),
    `import {captureReporter} from './capture.mjs'; export default {test:{reporters:['default',captureReporter(${JSON.stringify(report)})],maxWorkers:1,fileParallelism:false,coverage:{reportsDirectory:${JSON.stringify(report)},autoAttachSubprocess:${mode === "autoattach"}}}};`,
  );
  const options = {
    tsConfig: "tsconfig.json",
    runnerConfig: "vitest.config.ts",
    coverage: true,
    coverageReporters: ["lcovonly"],
    include: ["projects/research/src/case.spec.ts"],
  };
  if (mode !== "defaults")
    options.coverageInclude = ["projects/research/src/**/*.ts"];
  writeFileSync(
    join(workspace, "angular.json"),
    JSON.stringify({
      version: 1,
      projects: {
        research: {
          projectType: "application",
          root: "projects/research",
          sourceRoot: "projects/research/src",
          architect: {
            build: {
              builder: "@angular/build:application",
              options: {
                browser: "projects/research/src/main.ts",
                tsConfig: "tsconfig.json",
                styles: [],
                assets: [],
              },
              configurations: {
                development: { optimization: false, sourceMap: true },
              },
              defaultConfiguration: "development",
            },
            test: { builder: "@angular/build:unit-test", options },
          },
        },
      },
    }),
  );
  const r = run(
    `angular-${mode}`,
    [
      join(root, "angular/node_modules/@angular/cli/bin/ng.js"),
      "test",
      "research",
      "--no-watch",
    ],
    {
      cwd: workspace,
      env: {
        ...process.env,
        NODE_V8_COVERAGE: "",
        COVERAGE_OUTPUT_FILE: "",
        NG_BUILD_MAX_WORKERS: "2",
        NG_CLI_ANALYTICS: "false",
      },
    },
  );
  results.at(-1).lcovExists = existsSync(join(report, "lcov.info"));
  cpSync(join(workspace, "angular.json"), join(report, "angular.json"));
  cpSync(join(workspace, "vitest.config.ts"), join(report, "vitest.config.ts"));
}
cpSync(join(workspace, "projects"), join(out, "angular-originals"), {
  recursive: true,
});
writeFileSync(join(out, "results.json"), JSON.stringify(results, null, 2));
console.log(JSON.stringify(results, null, 2));

const authoritative = join(out, "angular-autoattach", "lcov.info");
if (process.env.COVERAGE_OUTPUT_FILE && existsSync(authoritative)) {
  const text = readFileSync(authoritative, "utf8").replace(
    /^SF:projects\/research\/src\//gm,
    "SF:tools/coverage/node_research/",
  );
  writeFileSync(process.env.COVERAGE_OUTPUT_FILE, text);
  writeFileSync(join(out, "authoritative-adapter.lcov"), text);
}
