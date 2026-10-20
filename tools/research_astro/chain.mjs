import { createRequire } from "node:module";
import { readFileSync, writeFileSync, mkdirSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import assert from "node:assert/strict";
const require = createRequire(import.meta.url);
const ar = createRequire(require.resolve("astro/package.json"));
const compiler = await import(ar.resolve("@astrojs/compiler-rs"));
const [serverDir, mergeDir, output] = process.argv
  .slice(2)
  .map((value) => resolve(value));
mkdirSync(output, { recursive: true });
const read = (file) => JSON.parse(readFileSync(file, "utf8"));
const save = (name, value) =>
  writeFileSync(
    resolve(output, name),
    typeof value === "string" ? value : JSON.stringify(value, null, 2),
  );
const ignored = new Set(["start", "end", "raw", "loc", "comments"]);
const position = (text, offset) => ({
  line: text.slice(0, offset).split("\n").length,
  column: offset - text.lastIndexOf("\n", offset - 1) - 1,
});
function walk(value, visit) {
  if (!value || typeof value !== "object") return;
  if (value.type) visit(value);
  for (const [key, child] of Object.entries(value))
    if (!ignored.has(key)) {
      if (Array.isArray(child)) child.forEach((entry) => walk(entry, visit));
      else walk(child, visit);
    }
}
function shape(value) {
  if (Array.isArray(value)) return value.map(shape);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(
    Object.entries(value)
      .filter(([key]) => !ignored.has(key))
      .map(([key, child]) => [key, shape(child)]),
  );
}
async function scriptAst(code) {
  const ast = (await compiler.parse(`<script>${code}</script>`)).ast.body[0]
    .children[0].program;
  walk(ast, (node) => {
    node.start -= 8;
    node.end -= 8;
  });
  return ast;
}
function scripts(ast) {
  const result = [];
  walk(ast, (node) => {
    if (node.type === "AstroScript") result.push(node.program);
  });
  return result;
}
function bridge(original, virtual, originalText, virtualText) {
  assert.deepEqual(
    shape(original.body),
    shape(virtual.body),
    "The compiler changed script structure. No mapping is inferred.",
  );
  const pairs = [];
  function pair(a, b) {
    if (!a || !b || typeof a !== "object" || typeof b !== "object") return;
    if (a.type && b.type)
      pairs.push({
        type: a.type,
        original: {
          start: position(originalText, a.start),
          end: position(originalText, a.end),
        },
        virtual: {
          start: position(virtualText, b.start),
          end: position(virtualText, b.end),
        },
      });
    for (const [key, child] of Object.entries(a))
      if (!ignored.has(key)) {
        if (Array.isArray(child))
          child.forEach((entry, index) => pair(entry, b[key]?.[index]));
        else pair(child, b[key]);
      }
  }
  pair(original.body, virtual.body);
  return pairs;
}
const inventory = {},
  bridgeRecords = {};
const statements = new Set([
  "VariableDeclaration",
  "ExpressionStatement",
  "ReturnStatement",
  "ThrowStatement",
  "IfStatement",
]);
for (const name of ["Main", "Disconnected", "TestOnly"]) {
  const source = readFileSync(
    resolve("tools/research_astro/fixtures", name + ".astro"),
    "utf8",
  );
  const ast = read(resolve(serverDir, name + ".ast.json")).ast;
  const lines = new Set();
  walk(ast, (node) => {
    if (statements.has(node.type) || node.type === "JSXExpressionContainer")
      lines.add(
        position(
          source,
          node.type === "JSXExpressionContainer"
            ? node.expression.start
            : node.start,
        ).line,
      );
  });
  inventory[name] = [...lines].sort((a, b) => a - b);
  const transformed = read(resolve(serverDir, name + ".compiler.json"));
  assert.equal(
    JSON.parse(transformed.map).sourcesContent[0],
    source,
    "The server map must contain the original component bytes.",
  );
  const processed = transformed.scripts;
  const originalScripts = scripts(ast);
  bridgeRecords[name] = [];
  for (let index = 0; index < processed.length; index++) {
    const virtual = await scriptAst(processed[index].code);
    bridgeRecords[name].push(
      bridge(originalScripts[index], virtual, source, processed[index].code),
    );
  }
}
const expected = {
  Main: [3, 4, 5, 8, 9, 10, 14, 15, 19],
  Disconnected: [2, 3, 5, 6],
  TestOnly: [2, 4],
};
assert.deepEqual(inventory, expected);
save("independent-expected-lines.json", expected);
save("compiler-ast-inventory.json", inventory);
save("script-bridge.json", bridgeRecords);

const browserArtifacts = resolve(mergeDir, "../browser");
const compiledMain = read(resolve(serverDir, "Main.compiler.json"));
const mapIdentity = [];
for (const filename of readdirSync(browserArtifacts))
  if (filename.endsWith(".map")) {
    const map = read(resolve(browserArtifacts, filename));
    for (const [index, source] of map.sources.entries())
      if (source.includes("Main.astro?")) {
        assert.equal(
          map.sourcesContent[index],
          compiledMain.scripts[0].code,
          "The bundle map must contain the exact compiler virtual script.",
        );
        mapIdentity.push({
          map: filename,
          virtualSource: source,
          exactCompilerText: true,
        });
      }
  }
assert.equal(mapIdentity.length, 1);
save("map-chain-identity.json", mapIdentity);
const virtualFiles = read(resolve(mergeDir, "virtual-unresolved.json"));
const canonical = read(resolve(mergeDir, "combined-zero-first.json"));
const processedLines = {};
for (const [file, coverage] of Object.entries(virtualFiles)) {
  const pairs = bridgeRecords.Main.flat();
  for (const [id, location] of Object.entries(coverage.statementMap)) {
    const matches = pairs.filter(
      (pair) =>
        pair.virtual.start.line === location.start.line &&
        pair.virtual.start.column === location.start.column &&
        statements.has(pair.type),
    );
    assert.equal(
      matches.length,
      1,
      "A virtual statement needs exactly one checked original statement.",
    );
    const line = matches[0].original.start.line;
    processedLines[line] = (processedLines[line] || 0) + coverage.s[id];
  }
}
assert.ok(processedLines[14] > 0 && processedLines[15] > 0);
save("processed-original-lines.json", processedLines);

function decodedMappings(map) {
  const alphabet =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  const rows = [];
  let source = 0,
    originalLine = 0,
    originalColumn = 0;
  for (const [line, text] of map.mappings.split(";").entries()) {
    let column = 0;
    for (const segment of text.split(",")) {
      if (!segment) continue;
      const fields = [];
      let value = 0,
        shift = 0;
      for (const character of segment) {
        const digit = alphabet.indexOf(character);
        assert.ok(digit >= 0);
        value |= (digit & 31) << shift;
        if (digit & 32) shift += 5;
        else {
          fields.push(value & 1 ? -(value >> 1) : value >> 1);
          value = shift = 0;
        }
      }
      column += fields[0];
      if (fields.length >= 4) {
        source += fields[1];
        originalLine += fields[2];
        originalColumn += fields[3];
        rows.push({
          line: line + 1,
          column,
          source,
          originalLine: originalLine + 1,
          originalColumn,
        });
      }
    }
  }
  return rows;
}
const templateEvidence = [];
const templateHits = {};
for (const name of ["Main", "TestOnly"]) {
  const source = readFileSync(
    resolve("tools/research_astro/fixtures", name + ".astro"),
    "utf8",
  );
  const ast = read(resolve(serverDir, name + ".ast.json")).ast;
  const compiled = read(resolve(serverDir, name + ".compiler.json"));
  const mappings = decodedMappings(JSON.parse(compiled.map));
  const generated = await scriptAst(compiled.code);
  const profile = read(resolve(serverDir, "server-profile.json")).result.find(
    (entry) => entry.url.endsWith(`/generated/${name}.mjs`),
  );
  assert.ok(profile);
  const ranges = profile.functions.flatMap((entry) => entry.ranges);
  const nodes = [];
  walk(generated, (node) => nodes.push(node));
  const containers = [];
  walk(ast, (node) => {
    if (node.type === "JSXExpressionContainer") containers.push(node);
  });
  for (const container of containers) {
    const expression = container.expression;
    const start = position(source, container.start),
      end = position(source, container.end);
    const candidates = nodes.filter((node) => {
      if (node.type !== expression.type) return false;
      if (expression.type === "Identifier" && node.name !== expression.name)
        return false;
      if (
        expression.type === "CallExpression" &&
        JSON.stringify(shape(node.callee)) !==
          JSON.stringify(shape(expression.callee))
      )
        return false;
      const point = position(compiled.code, node.start);
      const map = mappings
        .filter((row) => row.line === point.line && row.column <= point.column)
        .at(-1);
      return (
        map &&
        map.source === 0 &&
        (map.originalLine > start.line ||
          (map.originalLine === start.line &&
            map.originalColumn >= start.column)) &&
        (map.originalLine < end.line ||
          (map.originalLine === end.line && map.originalColumn <= end.column))
      );
    });
    if (candidates.length !== 1) {
      templateEvidence.push({
        name,
        start,
        end,
        candidates: candidates.length,
        status: "unresolved",
      });
      continue;
    }
    const node = candidates[0];
    const covering = ranges
      .filter(
        (range) =>
          range.startOffset <= node.start && node.start < range.endOffset,
      )
      .sort(
        (a, b) => a.endOffset - a.startOffset - (b.endOffset - b.startOffset),
      );
    assert.ok(covering.length);
    const line = position(source, expression.start).line;
    templateHits[name] ||= {};
    templateHits[name][line] = Math.max(
      templateHits[name][line] || 0,
      covering[0].count,
    );
    templateEvidence.push({
      name,
      start,
      end,
      generated: { start: node.start, end: node.end, type: node.type },
      selectedRange: covering[0],
      line,
      count: covering[0].count,
      status: "unique",
    });
  }
}
save("template-expression-evidence.json", templateEvidence);

const originalBranches = {};
const defaultArguments = [];
for (const name of ["Main", "Disconnected", "TestOnly"]) {
  const source = readFileSync(
    resolve("tools/research_astro/fixtures", name + ".astro"),
    "utf8",
  );
  const ast = read(resolve(serverDir, name + ".ast.json")).ast;
  const file = `tools/research_astro/fixtures/${name}.astro`;
  const coverage = canonical[file];
  originalBranches[file] = [];
  walk(ast, (node) => {
    if (node.type === "ConditionalExpression") {
      const line = position(source, node.start).line;
      const matches = Object.entries(coverage.branchMap).filter(
        ([, branch]) => branch.line === line && branch.type === "cond-expr",
      );
      assert.equal(
        matches.length,
        1,
        "Each original conditional needs one observed branch region.",
      );
      const [id] = matches[0];
      const counts = coverage.b[id];
      assert.equal(counts.length, 2);
      originalBranches[file].push({
        line,
        block: node.start,
        originalEnd: node.end,
        counts,
      });
    }
  });
  for (const [id, branch] of Object.entries(coverage.branchMap))
    if (branch.type === "default-arg")
      defaultArguments.push({
        file,
        line: branch.line,
        converterCounts: coverage.b[id],
        independentExpectedFallbackCount: 0,
      });
}
const branchOracle = {
  Main: { 9: [1, 1], 19: [1, 0] },
  Disconnected: { 5: [0, 0] },
  TestOnly: { 4: [1, 0] },
};
for (const [file, branches] of Object.entries(originalBranches)) {
  const name = file.split("/").at(-1).replace(".astro", "");
  for (const branch of branches)
    assert.deepEqual(branch.counts, branchOracle[name][branch.line]);
}
save("independent-expected-branches.json", branchOracle);
save("original-conditional-records.json", originalBranches);
save("default-argument-gap.json", defaultArguments);
const browserDir = resolve(mergeDir, "../browser");
assert.equal(
  read(resolve(browserDir, "maintained-functional.json")).passed,
  true,
);
assert.equal(
  read(resolve(browserDir, "maintained-completion.json")).completed,
  true,
);
const maintained = read(resolve(browserDir, "maintained-profile.json"));
assert.ok(maintained.result.length > 0);
const maintainedSummary = [];
for (const entry of maintained.result) {
  const text = readFileSync(resolve(browserDir, entry.textFile), "utf8");
  assert.ok(text.length > 0);
  maintainedSummary.push({
    url: entry.url,
    scriptId: entry.scriptId,
    textFile: entry.textFile,
    ranges: entry.functions.reduce((sum, f) => sum + f.ranges.length, 0),
    sourceMapURL: entry.parsed?.sourceMapURL || null,
  });
}
save("maintained-summary.json", maintainedSummary);
const measured = {};
for (const name of Object.keys(expected)) {
  const file = `tools/research_astro/fixtures/${name}.astro`;
  const coverage = canonical[file];
  const lines = Object.fromEntries(expected[name].map((line) => [line, 0]));
  for (const [id, location] of Object.entries(coverage?.statementMap || {}))
    if (location.start.line in lines)
      lines[location.start.line] += coverage.s[id];
  if (name === "Main")
    for (const [line, count] of Object.entries(processedLines))
      lines[line] += count;
  for (const [line, count] of Object.entries(templateHits[name] || {}))
    lines[line] = Math.max(lines[line] || 0, count);
  measured[file] = lines;
}
function lcov(map) {
  return Object.entries(map)
    .sort()
    .map(
      ([file, lines]) =>
        `SF:${file}\n` +
        Object.entries(lines)
          .map(([line, count]) => `DA:${line},${count}\n`)
          .join("") +
        "end_of_record\n",
    )
    .join("");
}
const baseline = Object.fromEntries(
  Object.entries(measured).map(([file, lines]) => [
    file,
    Object.fromEntries(Object.keys(lines).map((line) => [line, 0])),
  ]),
);
function merge(inputs) {
  const result = {};
  for (const input of inputs)
    for (const [file, lines] of Object.entries(input)) {
      result[file] ||= {};
      for (const [line, count] of Object.entries(lines))
        result[file][line] = Math.max(result[file][line] || 0, count);
    }
  return result;
}
const forward = lcov(merge([baseline, measured])),
  reverse = lcov(merge([measured, baseline]));
assert.equal(forward, reverse);

function withBranches(text, zero = false) {
  return text.replace(
    /SF:([^\n]+)\n([\s\S]*?)end_of_record\n/g,
    (_all, file, body) =>
      `SF:${file}\n${body}` +
      (originalBranches[file] || [])
        .flatMap((branch) =>
          branch.counts.map(
            (count, arm) =>
              `BRDA:${branch.line},${branch.block},${arm},${zero ? 0 : count}\n`,
          ),
        )
        .join("") +
      "end_of_record\n",
  );
}
save("original-baseline.lcov", withBranches(lcov(baseline), true));
save("partial-forward.lcov", withBranches(forward));
save("partial-reverse.lcov", withBranches(reverse));
const originalSource = readFileSync(
  resolve("tools/research_astro/fixtures/Main.astro"),
  "utf8",
);
const typedSource = originalSource.replace(
  'document.querySelector("#action").addEventListener',
  'document.querySelector<HTMLButtonElement>("#action").addEventListener',
);
const typedAst = (await compiler.parse(typedSource)).ast;
const typedCompile = await compiler.transform(typedSource, {
  filename: resolve("tools/research_astro/fixtures/Typed.astro"),
  sourcemap: "external",
});
let typedBridgeRejected = false,
  typedError;
try {
  bridge(
    scripts(typedAst)[0],
    await scriptAst(typedCompile.scripts[0].code),
    typedSource,
    typedCompile.scripts[0].code,
  );
} catch (error) {
  typedBridgeRejected = true;
  typedError = String(error);
}

let semanticMismatchRejected = false;
try {
  bridge(
    scripts((await compiler.parse(originalSource)).ast)[0],
    await scriptAst(
      compiledMain.scripts[0].code.replace(
        "browserLabel(true)",
        "browserLabel(false)",
      ),
    ),
    originalSource,
    compiledMain.scripts[0].code,
  );
} catch {
  semanticMismatchRejected = true;
}
assert.equal(semanticMismatchRejected, true);
save("typed-source.astro.txt", typedSource);
save("typed-compiler.json", typedCompile);
save("summary.json", {
  oracleMatchesAst: true,
  processedOriginalHits: processedLines,
  templateHits,
  mergeOrdersEqual: true,
  commentExcluded: true,
  staticHtmlExcluded: true,
  conditionalBranches: originalBranches,
  defaultArgumentGap: defaultArguments,
  missingMeasuredTemplateLines: [8, 9].filter(
    (line) => measured["tools/research_astro/fixtures/Main.astro"][line] === 0,
  ),
  typedBridgeRejected,
  typedError,
  semanticMismatchRejected,
  completeOriginalCoverage: false,
});
console.log(
  JSON.stringify({
    processedOriginalHits: processedLines,
    typedBridgeRejected,
    completeOriginalCoverage: false,
  }),
);
