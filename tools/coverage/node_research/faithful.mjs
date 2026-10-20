import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  realpathSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
const root = process.cwd();
const here = dirname(fileURLToPath(import.meta.url));
const out = process.env.TEST_UNDECLARED_OUTPUTS_DIR;
const tmp = process.env.TEST_TMPDIR;
mkdirSync(out, { recursive: true });
const providerRequire = createRequire(
  realpathSync(
    join(root, "angular/node_modules/@vitest/coverage-v8/package.json"),
  ),
);
const { default: convert } = await import(
  pathToFileURL(providerRequire.resolve("ast-v8-to-istanbul"))
);
const { parseAstAsync } = await import(
  pathToFileURL(providerRequire.resolve("vitest/node"))
);
const coverage = providerRequire("@vitest/istanbul-lib-coverage");
const ts = (
  await import(
    pathToFileURL(
      join(root, "angular/node_modules/typescript/lib/typescript.js"),
    )
  )
).default;
const save = (name, value) =>
  writeFileSync(join(out, name), JSON.stringify(value, null, 2));
const sha = (text) => createHash("sha256").update(text).digest("hex");
const rel = "tools/coverage/node_research";
const inventory = readdirSync(here)
  .filter((x) =>
    [
      "production.ts",
      "helper.ts",
      "disconnected.ts",
      "child_only.ts",
      "lowered.ts",
    ].includes(x),
  )
  .map((name) => ({
    path: `${rel}/${name}`,
    text: readFileSync(join(here, name), "utf8"),
  }));
const maintained = JSON.parse(
  readFileSync(join(here, "maintained/sources.json"), "utf8"),
).map((x) => ({
  ...x,
  path: x.source,
  text: readFileSync(join(here, "maintained", x.snapshot), "utf8"),
}));
for (const item of maintained) assert.equal(sha(item.text), item.sha256);
const angularTestInput = {
  path: rel + "/angular-case.spec.ts",
  text: readFileSync(join(here, "angular-case.spec.ts"), "utf8"),
};
const angularChildInput = {
  path: rel + "/angular-child.mjs",
  text: readFileSync(join(here, "angular-child.mjs"), "utf8"),
};
const trackedInputs = [
  ...inventory,
  ...maintained,
  angularTestInput,
  angularChildInput,
];
const angularCaseExpected = [9, 10, 11, 12, 15, 16, 17, 18, 19, 24];
const nativeInventory = [
  ...inventory,
  ...maintained.filter((x) => !x.path.startsWith("angular/")),
];
save(
  "inventory.json",
  trackedInputs.map(({ path, text }) => ({
    path,
    sha256: sha(text),
  })),
);
function originalModel(path, text) {
  const source = ts.createSourceFile(path, text, ts.ScriptTarget.Latest, true);
  const lines = new Set();
  const branches = [];
  const line = (node) =>
    source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1;
  function visit(node) {
    if (ts.isVariableDeclaration(node) && node.initializer)
      lines.add(line(node));
    if (
      ts.isReturnStatement(node) ||
      ts.isExpressionStatement(node) ||
      ts.isThrowStatement(node) ||
      ts.isIfStatement(node) ||
      ts.isForOfStatement(node)
    )
      lines.add(line(node));
    if (ts.isArrowFunction(node) && !ts.isBlock(node.body))
      lines.add(line(node.body));
    if (ts.isIfStatement(node) || ts.isConditionalExpression(node))
      branches.push({
        line: line(node),
        arms: 2,
        kind: ts.SyntaxKind[node.kind],
      });
    if (
      ts.isBinaryExpression(node) &&
      [
        ts.SyntaxKind.BarBarToken,
        ts.SyntaxKind.AmpersandAmpersandToken,
        ts.SyntaxKind.QuestionQuestionToken,
      ].includes(node.operatorToken.kind)
    )
      branches.push({
        line: line(node),
        arms: 2,
        kind: ts.SyntaxKind[node.operatorToken.kind],
      });
    ts.forEachChild(node, visit);
  }
  visit(source);
  return { path, lines: [...lines].sort((a, b) => a - b), branches };
}
const models = trackedInputs.map((x) => originalModel(x.path, x.text));
save("original-ast-models.json", models);
assert.deepEqual(
  models.find((x) => x.path === angularTestInput.path).lines,
  angularCaseExpected,
);
save("angular-test-expectations.json", {
  path: angularTestInput.path,
  sha256: sha(angularTestInput.text),
  lines: angularCaseExpected,
});
const explicit = {
  [`${rel}/production.ts`]: { lines: [7, 8, 10], branches: 2 },
  [`${rel}/helper.ts`]: { lines: [2], branches: 0 },
  [`${rel}/disconnected.ts`]: { lines: [7, 8, 10], branches: 2 },
  [`${rel}/child_only.ts`]: { lines: [2], branches: 0 },
  [`${rel}/lowered.ts`]: { lines: [3], branches: 2 },
  "personal_website/src/utils/seo.ts": {
    lines: [2, 3, 4, 5, 6, 7, 10],
    branches: 4,
  },
};
save("independent-expectations.json", explicit);
const native = join(tmp, "native");
const originals = join(native, "originals");
const emitted = join(native, "emitted");
const profiles = join(native, "profiles");
mkdirSync(profiles, { recursive: true });
const emittedInputs = [];
for (const item of nativeInventory) {
  const original = join(originals, item.path);
  mkdirSync(dirname(original), { recursive: true });
  writeFileSync(original, item.text);
  const generated = join(emitted, item.path.replace(/\.ts$/, ".js"));
  mkdirSync(dirname(generated), { recursive: true });
  const result = ts.transpileModule(item.text, {
    fileName: original,
    compilerOptions: {
      target: item.path.endsWith("/lowered.ts")
        ? ts.ScriptTarget.ES5
        : ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.ESNext,
      sourceMap: true,
      inlineSources: true,
    },
  });
  const map = JSON.parse(result.sourceMapText);
  map.sources = [original];
  map.sourcesContent = [item.text];
  writeFileSync(generated, result.outputText);
  writeFileSync(generated + ".map", JSON.stringify(map));
  emittedInputs.push({
    ...item,
    original,
    generated,
    code: result.outputText,
    map,
  });
}
writeFileSync(join(native, "package.json"), ' {"type":"module"}');
const runner = join(native, "parent.mjs");
const child = join(native, "child.mjs");
writeFileSync(
  child,
  `import assert from 'node:assert/strict';import {childOnly} from ${JSON.stringify(pathToFileURL(join(emitted, rel, "child_only.js")).href)};import {canonicalUrl,escapeXml} from ${JSON.stringify(pathToFileURL(join(emitted, "personal_website/src/utils/seo.js")).href)};assert.equal(childOnly(),41);assert.equal(canonicalUrl('/post?x=1',new URL('https://example.test/')),'https://example.test/post/');assert.equal(escapeXml('<&'), '&lt;&amp;');`,
);
writeFileSync(
  runner,
  `import assert from 'node:assert/strict';import {spawnSync} from 'node:child_process';import ${JSON.stringify(pathToFileURL(join(emitted, "personal_website/tests/blog.test.js")).href)};import {featuredPosts} from ${JSON.stringify(pathToFileURL(join(emitted, "personal_website/tests/fixtures/posts.js")).href)};import {choose} from ${JSON.stringify(pathToFileURL(join(emitted, rel, "production.js")).href)};import {twice} from ${JSON.stringify(pathToFileURL(join(emitted, rel, "helper.js")).href)};import {lowered} from ${JSON.stringify(pathToFileURL(join(emitted, rel, "lowered.js")).href)};assert.equal(featuredPosts.length,3);assert.equal(choose(1),2);assert.equal(twice(3),6);assert.equal(await lowered(true),9);const child=spawnSync(process.execPath,[${JSON.stringify(child)}],{encoding:'utf8'});assert.equal(child.status,0,child.stderr);console.log(JSON.stringify({functional:true,parentPid:process.pid,childPid:child.pid}));`,
);
const functional = spawnSync(process.execPath, [runner], {
  encoding: "utf8",
  timeout: 120000,
  env: { ...process.env, NODE_OPTIONS: "", NODE_V8_COVERAGE: profiles },
});
writeFileSync(join(out, "native-functional.stdout"), functional.stdout ?? "");
writeFileSync(join(out, "native-functional.stderr"), functional.stderr ?? "");
assert.equal(functional.status, 0, functional.stderr);
const rawProfiles = readdirSync(profiles)
  .filter((x) => x.endsWith(".json"))
  .map((name) => ({
    name,
    ...JSON.parse(readFileSync(join(profiles, name), "utf8")),
  }));
const collected = coverage.createCoverageMap({});
const baseline = coverage.createCoverageMap({});
const perProcess = [];
for (const profile of rawProfiles) {
  const processMap = coverage.createCoverageMap({});
  for (const item of emittedInputs) {
    const match = profile.result.find(
      (script) => script.url === pathToFileURL(item.generated).href,
    );
    if (match) {
      const converted = await convert({
        code: item.code,
        sourceMap: item.map,
        ast: await parseAstAsync(item.code),
        coverage: match,
      });
      processMap.merge(converted);
    }
  }
  perProcess.push({ profile: profile.name, coverage: processMap.toJSON() });
  collected.merge(processMap);
}
for (const item of emittedInputs)
  baseline.merge(
    await convert({
      code: item.code,
      sourceMap: item.map,
      ast: await parseAstAsync(item.code),
      coverage: { url: pathToFileURL(item.generated).href, functions: [] },
    }),
  );
const canonical = (path) => relative(originals, path).replaceAll("\\", "/");
function mapRecords(map) {
  return map.files().map((path) => {
    const file = map.fileCoverageFor(path);
    const branches = Object.entries(file.branchMap).flatMap(([id, branch]) =>
      branch.locations.map((location, arm) => ({
        line: location.start.line ?? branch.line ?? branch.loc.start.line,
        type: branch.type,
        id: Number(id),
        arm,
        hits: file.b[id][arm],
      })),
    );
    return { path: canonical(path), lines: file.getLineCoverage(), branches };
  });
}
function projectedRecords(map) {
  return mapRecords(map).map((record) => {
    const model = models.find((x) => x.path === record.path);
    assert(model, "An original source has no independent AST model.");
    for (const line of model.lines)
      assert(
        line in record.lines,
        `Missing mapped executable line: ${record.path}:${line}`,
      );
    return {
      ...record,
      lines: Object.fromEntries(
        Object.entries(record.lines).filter(([line]) =>
          model.lines.includes(Number(line)),
        ),
      ),
    };
  });
}
function lcov(map, projected = false) {
  return (projected ? projectedRecords(map) : mapRecords(map))
    .sort((a, b) => a.path.localeCompare(b.path))
    .map(
      (x) =>
        `SF:${x.path}\n` +
        Object.entries(x.lines)
          .sort((a, b) => Number(a[0]) - Number(b[0]))
          .map(([l, h]) => `DA:${l},${h}\n`)
          .join("") +
        x.branches
          .map((b) => `BRDA:${b.line},${b.id},${b.arm},${b.hits}\n`)
          .join("") +
        "end_of_record\n",
    )
    .join("");
}
const branchVerdicts = [
  { path: rel + "/production.ts", line: 7, type: "if" },
  { path: rel + "/lowered.ts", line: 3, type: "cond-expr" },
].map((expected) => {
  const branches = mapRecords(collected).find(
    (record) => record.path === expected.path,
  ).branches;
  const passed =
    branches.length === 2 &&
    branches.every((branch) => branch.type === expected.type) &&
    branches[0].hits > 0 &&
    branches[1].hits === 0;
  return {
    ...expected,
    expectedArmMeaning: ["true-positive", "false-zero"],
    actual: branches,
    passed,
  };
});
save("native-branch-expectations.json", branchVerdicts);
for (const record of mapRecords(baseline))
  for (const branch of record.branches)
    assert(
      Number.isInteger(branch.line) && branch.line > 0,
      "A branch has no valid original anchor.",
    );
save("native-measured.json", mapRecords(collected));
save("native-baseline.json", mapRecords(baseline));
save("native-processes.json", perProcess);
writeFileSync(join(out, "native-measured.lcov"), lcov(collected));
writeFileSync(join(out, "native-baseline.lcov"), lcov(baseline));
const forward = coverage.createCoverageMap(
  JSON.parse(JSON.stringify(baseline.toJSON())),
);
forward.merge(collected);
const reverse = coverage.createCoverageMap(
  JSON.parse(JSON.stringify(collected.toJSON())),
);
reverse.merge(baseline);
writeFileSync(join(out, "native-combined-forward.lcov"), lcov(forward));
writeFileSync(join(out, "native-combined-reverse.lcov"), lcov(reverse));
for (const [name, map] of [
  ["baseline", baseline],
  ["measured", collected],
  ["combined-forward", forward],
  ["combined-reverse", reverse],
])
  writeFileSync(join(out, `native-projected-${name}.lcov`), lcov(map, true));
save(
  "native-projection.json",
  mapRecords(collected).map((record) => ({
    path: record.path,
    removed: Object.entries(record.lines)
      .filter(
        ([line]) =>
          !models
            .find((x) => x.path === record.path)
            .lines.includes(Number(line)),
      )
      .map(([line, hits]) => ({ line: Number(line), hits })),
    retained: projectedRecords(collected).find((x) => x.path === record.path)
      .lines,
  })),
);

const comparisons = mapRecords(baseline).map((record) => {
  const model = models.find((x) => x.path === record.path);
  const actual = Object.keys(record.lines)
    .map(Number)
    .sort((a, b) => a - b);
  return {
    path: record.path,
    expected: explicit[record.path] ?? model,
    actualLines: actual,
    actualBranchArms: record.branches.length,
    missingLines: model.lines.filter((x) => !actual.includes(x)),
    extraLines: actual.filter((x) => !model.lines.includes(x)),
    explicitLineMatch: explicit[record.path]
      ? JSON.stringify(actual) === JSON.stringify(explicit[record.path].lines)
      : null,
  };
});
save("native-comparisons.json", comparisons);
cpSync(native, join(out, "native-inputs-and-profiles"), { recursive: true });
const workspace = join(tmp, "angular-workspace");
const src = join(workspace, "projects/research/src");
mkdirSync(src, { recursive: true });
const workspaceModules = join(workspace, "node_modules");
mkdirSync(workspaceModules, { recursive: true });
const linkDiagnostics = [];
for (const name of readdirSync(join(root, "angular/node_modules"))) {
  const path = join(root, "angular/node_modules", name);
  const packages = name.startsWith("@")
    ? readdirSync(path).map((child) => name + "/" + child)
    : [name];
  for (const packageName of packages) {
    const input = join(root, "angular/node_modules", packageName);
    const target = join(workspaceModules, packageName);
    try {
      const actual = realpathSync(input);
      mkdirSync(dirname(target), { recursive: true });
      symlinkSync(actual, target, "dir");
      linkDiagnostics.push({
        packageName,
        actual,
        valid: existsSync(join(target, "package.json")),
      });
    } catch (error) {
      linkDiagnostics.push({ packageName, error: error.message });
    }
  }
}
save("angular-package-links.json", linkDiagnostics);
for (const name of ["capture.mjs", "probe-reporter.mjs"])
  cpSync(join(here, name), join(workspace, name));
writeFileSync(join(workspace, "package.json"), '{"type":"module"}');
writeFileSync(join(src, "main.ts"), "export {};\n");
for (const item of inventory)
  writeFileSync(join(src, item.path.split("/").at(-1)), item.text);
for (const item of maintained) {
  const path = join(src, "maintained", item.path);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, item.text);
}
const spec = readFileSync(join(here, "angular-case.spec.ts"), "utf8");
writeFileSync(join(src, "case.spec.ts"), spec);
cpSync(join(here, "angular-child.mjs"), join(src, "child.mjs"));
writeFileSync(
  join(workspace, "astro.d.ts"),
  'declare module "astro:content" {export type CollectionEntry<T>={id:string;body?:string;data:{title:string;description:string;tags:string[];publishDate:Date;featured?:boolean;draft?:boolean;[key:string]:unknown}};}',
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
    include: ["projects/**/*.ts", "astro.d.ts"],
  }),
);
const angularResults = [];
for (const includeTests of [false, true]) {
  const mode = includeTests ? "include-test-metadata" : "default-test-filter";
  const destination = join(out, "angular-" + mode);
  mkdirSync(destination, { recursive: true });
  writeFileSync(
    join(workspace, "vitest.config.ts"),
    `import {probeReporter} from './probe-reporter.mjs';export default {test:{reporters:['default',probeReporter(${JSON.stringify(destination)},${includeTests},${JSON.stringify(workspace)})],maxWorkers:1,fileParallelism:false,coverage:{exclude:[],reportsDirectory:${JSON.stringify(destination)},autoAttachSubprocess:true}}};`,
  );
  const options = {
    tsConfig: "tsconfig.json",
    runnerConfig: "vitest.config.ts",
    coverage: true,
    coverageReporters: ["lcovonly"],
    coverageInclude: ["projects/research/src/**/*.ts"],
    include: ["projects/research/src/case.spec.ts"],
  };
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
  const run = spawnSync(
    process.execPath,
    [
      join(root, "angular/node_modules/@angular/cli/bin/ng.js"),
      "test",
      "research",
      "--no-watch",
    ],
    {
      cwd: workspace,
      encoding: "utf8",
      timeout: 180000,
      env: {
        ...process.env,
        NODE_OPTIONS: "",
        NODE_V8_COVERAGE: "",
        COVERAGE_OUTPUT_FILE: "",
        NG_BUILD_MAX_WORKERS: "2",
        NG_CLI_ANALYTICS: "false",
        RESEARCH_CHILD_SCRIPT: join(src, "child.mjs"),
      },
    },
  );
  writeFileSync(join(destination, "stdout"), run.stdout ?? "");
  writeFileSync(join(destination, "stderr"), run.stderr ?? "");
  cpSync(join(workspace, "angular.json"), join(destination, "angular.json"));
  cpSync(
    join(workspace, "vitest.config.ts"),
    join(destination, "vitest.config.ts"),
  );
  angularResults.push({
    mode,
    status: run.status,
    signal: run.signal,
    error: run.error?.message,
    lcov: existsSync(join(destination, "lcov.info")),
  });
}
cpSync(join(workspace, "projects"), join(out, "angular-originals"), {
  recursive: true,
});
save("angular-results.json", angularResults);
const canonicalMapping = Object.fromEntries(
  inventory.map((item) => [
    "projects/research/src/" + item.path.split("/").at(-1),
    item.path,
  ]),
);
for (const item of maintained)
  canonicalMapping["projects/research/src/maintained/" + item.path] = item.path;
canonicalMapping["projects/research/src/case.spec.ts"] =
  rel + "/angular-case.spec.ts";
canonicalMapping["projects/research/src/child.mjs"] =
  rel + "/angular-child.mjs";
save("angular-canonical-mapping.json", canonicalMapping);
for (const row of angularResults) {
  const directory = join(out, "angular-" + row.mode);
  if (existsSync(join(directory, "lcov.info"))) {
    const blocks = readFileSync(join(directory, "lcov.info"), "utf8")
      .split("end_of_record")
      .filter((block) => block.includes("SF:"));
    const skipped = [];
    const normalized = blocks
      .map((block) => {
        const path = block.match(/^SF:(.*)$/m)[1];
        if (!canonicalMapping[path]) {
          assert(
            !/^DA:/m.test(block) && !/^BRDA:/m.test(block),
            "An executable source has no canonical identity.",
          );
          skipped.push({ path, reason: "no-executable-records" });
          return "";
        }
        return (
          block.replace(/^SF:.*$/m, "SF:" + canonicalMapping[path]).trim() +
          "\nend_of_record\n"
        );
      })
      .join("");
    writeFileSync(join(directory, "canonical.lcov"), normalized);
    writeFileSync(
      join(directory, "non-executable-records.json"),
      JSON.stringify(skipped, null, 2),
    );
  }
}
const positiveExpectations = {
  [rel + "/production.ts"]: [8],
  [rel + "/helper.ts"]: [2],
  [rel + "/child_only.ts"]: [2],
  "personal_website/src/utils/blog.ts": [6],
  "personal_website/tests/blog.test.ts": [37],
  "personal_website/src/utils/seo.ts": [2],
};
function validateNative(maps) {
  const combined = coverage.createCoverageMap({});
  for (const map of maps)
    combined.merge(JSON.parse(JSON.stringify(map.coverage)));
  const records = mapRecords(combined);
  for (const [path, lines] of Object.entries(positiveExpectations))
    for (const line of lines)
      assert(
        (records.find((x) => x.path === path)?.lines[line] ?? 0) > 0,
        `Missing expected measured hit: ${path}:${line}`,
      );
}
const nativeFaults = [];
for (const [mode, maps] of [
  ["valid", perProcess],
  ["missing", []],
  [
    "partial",
    perProcess.filter(
      (map) =>
        !mapRecords(
          coverage.createCoverageMap(JSON.parse(JSON.stringify(map.coverage))),
        ).some((record) => record.path === rel + "/child_only.ts"),
    ),
  ],
]) {
  let accepted = true,
    error;
  try {
    validateNative(maps);
  } catch (failure) {
    accepted = false;
    error = failure.message;
  }
  nativeFaults.push({
    mode,
    functionalStatus: functional.status,
    accepted,
    error,
  });
}
save("native-collection-validation.json", nativeFaults);
assert(nativeFaults.find((row) => row.mode === "valid").accepted);
assert(
  nativeFaults
    .filter((row) => row.mode !== "valid")
    .every((row) => !row.accepted),
);

const adapterResults = [];
function parseLcov(text) {
  const records = {};
  let path;
  for (const line of text.split("\n")) {
    if (line.startsWith("SF:")) {
      path = line.slice(3);
      records[path] = {};
    }
    if (line.startsWith("DA:")) {
      const [number, hits] = line.slice(3).split(",");
      assert(path && number && hits);
      records[path][number] = Number(hits);
    }
  }
  return records;
}
function independentlyValidate(file, expected) {
  assert(existsSync(file), "missing-output");
  const text = readFileSync(file, "utf8");
  assert(text.length, "empty-output");
  const records = parseLcov(text);
  for (const [path, lines] of Object.entries(expected))
    for (const line of lines)
      assert(
        line in (records[path] ?? {}),
        `missing-executable:${path}:${line}`,
      );
  return true;
}
const valid = readFileSync(
  join(out, "angular-include-test-metadata/lcov.info"),
  "utf8",
);
for (const project of [
  "mindreadr",
  "nicknamer",
  "nicknamer2-web",
  "pinyin-composer",
  "predix",
  "tailwind-sample",
]) {
  for (const mode of ["missing", "partial", "valid"]) {
    const area = join(tmp, project + "-" + mode);
    const config = join(area, "config");
    const writable = join(area, "tmp");
    mkdirSync(config, { recursive: true });
    mkdirSync(writable, { recursive: true });
    const adapter = readFileSync(
      join(here, "adapters", project + ".input"),
      "utf8",
    );
    writeFileSync(join(config, "setup.mjs"), adapter);
    const report = join(
      ["predix", "tailwind-sample"].includes(project) ? config : writable,
      ".vitest-coverage",
    );
    if (mode !== "missing") {
      mkdirSync(report, { recursive: true });
      writeFileSync(
        join(report, "lcov.info"),
        mode === "valid"
          ? valid
          : "SF:projects/research/src/production.ts\nDA:8,1\nend_of_record\n",
      );
    }
    const destination = join(writable, "coverage.dat");
    const run = spawnSync(
      process.execPath,
      [
        "--input-type=module",
        "-e",
        `const adapter=await import(${JSON.stringify(pathToFileURL(join(config, "setup.mjs")).href)});await adapter.setup?.();await adapter.teardown();`,
      ],
      {
        encoding: "utf8",
        env: {
          ...process.env,
          NODE_OPTIONS: "",
          NODE_V8_COVERAGE: "",
          TEST_TMPDIR: writable,
          COVERAGE_OUTPUT_FILE: destination,
        },
      },
    );
    let validation = true,
      error;
    try {
      independentlyValidate(destination, {
        "angular/projects/research/src/production.ts": [7, 8, 10],
        "angular/projects/research/src/helper.ts": [2],
        "angular/projects/research/src/disconnected.ts": [7, 8, 10],
        "angular/projects/research/src/child_only.ts": [2],
        "angular/projects/research/src/case.spec.ts": angularCaseExpected,
      });
    } catch (failure) {
      validation = false;
      error = failure.message;
    }
    adapterResults.push({
      project,
      mode,
      functionalStatus: angularResults.at(-1).status,
      adapterStatus: run.status,
      outputExists: existsSync(destination),
      independentAccepted: validation,
      error,
    });
    const saved = join(out, "adapter-" + project + "-" + mode);
    mkdirSync(saved, { recursive: true });
    writeFileSync(join(saved, "stdout"), run.stdout ?? "");
    writeFileSync(join(saved, "stderr"), run.stderr ?? "");
    if (existsSync(destination))
      cpSync(destination, join(saved, "output.lcov"));
  }
}
save("adapter-results.json", adapterResults);
save("environment.json", {
  node: process.version,
  typescript: ts.version,
  converter: JSON.parse(
    readFileSync(
      join(
        dirname(providerRequire.resolve("ast-v8-to-istanbul")),
        "../package.json",
      ),
      "utf8",
    ),
  ).version,
  vitest: JSON.parse(
    readFileSync(providerRequire.resolve("vitest/package.json"), "utf8"),
  ).version,
  functionalStatus: functional.status,
  rawProfileCount: rawProfiles.length,
  sameNativeMergeBytes: lcov(forward) === lcov(reverse),
});
console.log(
  JSON.stringify(
    {
      functionalStatus: functional.status,
      angularResults,
      adapterResults,
      nativeComparisons: comparisons,
    },
    null,
    2,
  ),
);
assert(
  angularResults.every((x) => x.status === 0),
  "An Angular functional experiment failed.",
);
assert(
  adapterResults
    .filter((x) => x.mode !== "valid")
    .every((x) => !x.independentAccepted),
  "A missing or partial adapter report passed independent validation.",
);

assert(
  adapterResults
    .filter((x) => x.mode === "valid")
    .every((x) => x.independentAccepted),
  "A valid complete adapter output failed independent validation.",
);
