import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import {
  command,
  requireCommand,
  canonical,
  counts,
  wasmSections,
} from "./collector.mjs";
import { createHash } from "node:crypto";
import { copyFile, unlink } from "node:fs/promises";

const [webPath, browserPath, runtimePath, cov, profdata, readobj, native] =
  process.argv.slice(2).map((value) => resolve(value));
const outputPath = process.env.TEST_UNDECLARED_OUTPUTS_DIR;
const collector = new URL("./collector.mjs", import.meta.url).pathname;
await mkdir(outputPath, { recursive: true });
const toolResults = [];
async function probe(name, executable, args, env) {
  const result = command(executable, args, env);
  await writeFile(join(outputPath, `${name}.stdout`), result.stdout);
  await writeFile(join(outputPath, `${name}.stderr`), result.stderr);
  toolResults.push({ name, status: result.status, error: result.error });
  return result;
}
const objects = {
  raw: join(runtimePath, "browser_fixture.wasm"),
  transformed: join(webPath, "fixture_bg.wasm"),
  ir: join(runtimePath, "fixture-ir.o"),
};
for (const [name, object] of Object.entries(objects)) {
  await probe(`baseline-${name}`, cov, [
    "export",
    "-format=lcov",
    "-empty-profile",
    object,
  ]);
  if (name !== "ir")
    await probe(`sections-${name}`, readobj, [
      "--sections",
      "--symbols",
      object,
    ]);
}
await writeFile(
  join(outputPath, "tool-versions.txt"),
  requireCommand(cov, ["--version"]) + requireCommand(profdata, ["--version"]),
);
const digestInputs = {
  ...objects,
  browser: browserPath,
  cov,
  profdata,
  readobj,
  native,
  node: process.execPath,
  minicov_rlib: join(runtimePath, "libminicov.rlib"),
  runtime_archive: join(runtimePath, "libllvm_profiler_runtime.a"),
  compiler_ir: join(runtimePath, "fixture.ll"),
};
await writeFile(
  join(outputPath, "module-digests.json"),
  JSON.stringify(
    Object.fromEntries(
      await Promise.all(
        Object.entries(digestInputs).map(async ([name, path]) => [
          name,
          createHash("sha256")
            .update(await readFile(path))
            .digest("hex"),
        ]),
      ),
    ),
    null,
    2,
  ),
);
for (const name of ["raw", "transformed"])
  await writeFile(
    join(outputPath, `wasm-sections-${name}.json`),
    JSON.stringify(wasmSections(await readFile(objects[name])), null, 2),
  );
const received = new Map();
const server = createServer(async (request, response) => {
  if (request.method === "POST") {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    received.set(request.url, Buffer.concat(chunks));
    response.end("ok");
    return;
  }
  const name = request.url === "/" ? "fixture.js" : request.url.slice(1);
  try {
    const body = await readFile(join(webPath, name));
    response.setHeader(
      "Content-Type",
      name.endsWith(".wasm") ? "application/wasm" : "text/javascript",
    );
    response.end(body);
  } catch {
    response.statusCode = 404;
    response.end();
  }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const origin = `http://127.0.0.1:${server.address().port}`;
let browser;
try {
  browser = await chromium.launch({
    executablePath: browserPath,
    headless: true,
    args: ["--no-sandbox"],
  });
  const results = [];
  for (const transfer of [true, false]) {
    const page = await browser.newPage();
    await page.goto(origin);
    const observation = await page.evaluate(
      async ({ transfer }) => {
        const { default: init } = await import("/fixture.js");
        const wasm = await init();
        const value = wasm.run();
        if (value !== 83) throw new Error(`Functional result ${value}`);
        if (transfer) {
          const size = wasm.dump_profile();
          const pointer = wasm.profile_pointer();
          if (size <= 0 || size > 65536)
            throw new Error(`Profile size ${size}`);
          const bytes = new Uint8Array(wasm.memory.buffer, pointer, size);
          await fetch("/profile", { method: "POST", body: bytes });
        }
        return { functional_pass: true, value, transfer };
      },
      { transfer },
    );
    const key = transfer ? "/profile" : "/suppressed";
    const profile = received.get(key);
    results.push({
      ...observation,
      collection_pass: Boolean(profile?.length),
      profile_bytes: profile?.length ?? 0,
    });
    if (transfer && !profile?.length)
      throw new Error("Missing browser profile");
    if (!transfer && profile?.length)
      throw new Error("Suppressed transfer produced a profile");
    if (profile) await writeFile(join(outputPath, "browser.profraw"), profile);
    await page.close();
  }
  await writeFile(
    join(outputPath, "browser.json"),
    JSON.stringify({ browser: browser.version(), results }, null, 2),
  );
  const browserProfile = join(outputPath, "browser.profraw");
  const indexed = join(outputPath, "browser.profdata");
  requireCommand(profdata, [
    "merge",
    "--sparse",
    browserProfile,
    "-o",
    indexed,
  ]);
  const functions = await probe("profile-functions", profdata, [
    "show",
    "--all-functions",
    "--counts",
    indexed,
  ]);
  const ir = await readFile(join(runtimePath, "fixture.ll"), "utf8");
  const correspondence = [];
  for (const line of ir
    .split("\n")
    .filter((line) => /^@__profd_.*subject/.test(line))) {
    const match = line.match(
      /^@__profd_(\S+).*\{ i64 (-?\d+), i64 (-?\d+),.*ptr null, ptr null, i32 (\d+),/,
    );
    if (!match) throw new Error("Unrecognized compiler profile record");
    const [, name, nameHash, functionHash, counters] = match;
    const hash = `0x${BigInt.asUintN(64, BigInt(functionHash)).toString(16)}`;
    const block = functions.stdout
      .split(`  ${name}:`)[1]
      ?.split(/\n(?=  \S)/)[0];
    const observed = Boolean(block);
    if (
      observed &&
      (!block.includes(`Hash: ${hash}`) ||
        !block.includes(`Counters: ${counters}`))
    )
      throw new Error("Compiler/profile identity mismatch");
    correspondence.push({
      name,
      name_hash: BigInt.asUintN(64, BigInt(nameHash)).toString(),
      function_hash: hash,
      counters: Number(counters),
      observed,
    });
  }
  if (correspondence.filter((record) => record.observed).length !== 2)
    throw new Error("Missing measured subject identities");
  const rawBytes = await readFile(browserProfile);
  await writeFile(
    join(outputPath, "profile-correspondence.json"),
    JSON.stringify(
      {
        raw_version_low32: Number(rawBytes.readBigUInt64LE(8) & 0xffffffffn),
        node: process.version,
        functions: correspondence,
      },
      null,
      2,
    ),
  );
  let selected;
  for (const [name, object] of Object.entries(objects)) {
    const result = await probe(`measured-${name}`, cov, [
      "export",
      "-format=lcov",
      `-instr-profile=${indexed}`,
      object,
    ]);
    if (result.status === 0) {
      try {
        const lcov = canonical(result.stdout);
        const hits = counts(lcov);
        if (hits[3] > 0 && hits[15] > 0 && hits[8] === 0 && !selected)
          selected = { name, object, lcov };
      } catch {}
    }
  }
  if (!selected)
    throw new Error("No mapping object preserved expected browser lines");
  const baseline = canonical(
    await readFile(
      join(outputPath, `baseline-${selected.name}.stdout`),
      "utf8",
    ),
  );
  if (
    Object.values(counts(baseline)).some((value) => value !== 0) ||
    counts(baseline)[8] !== 0
  )
    throw new Error("Invalid empty baseline");
  await writeFile(join(outputPath, "baseline.dat"), baseline);
  await writeFile(join(outputPath, "wasm.dat"), selected.lcov);
  await probe("native-functional", native, [], {
    ...process.env,
    LLVM_PROFILE_FILE: join(outputPath, "native.profraw"),
  });
  requireCommand(profdata, [
    "merge",
    "--sparse",
    join(outputPath, "native.profraw"),
    "-o",
    join(outputPath, "native.profdata"),
  ]);
  const nativeLcov = canonical(
    requireCommand(cov, [
      "export",
      "-format=lcov",
      `-instr-profile=${join(outputPath, "native.profdata")}`,
      native,
    ]),
  );
  if (!(counts(nativeLcov)[19] > 0)) throw new Error("Missing native-only hit");
  await writeFile(join(outputPath, "native.dat"), nativeLcov);
  await copyFile(browserProfile, join(outputPath, "missing.profraw"));
  await unlink(join(outputPath, "missing.profraw"));
  await copyFile(selected.object, join(outputPath, "missing-mapping.wasm"));
  await unlink(join(outputPath, "missing-mapping.wasm"));
  await writeFile(join(outputPath, "corrupt.profraw"), "corrupt profile");
  for (const [name, profile, mapping] of [
    ["missing-profile", join(outputPath, "missing.profraw"), selected.object],
    ["corrupt-profile", join(outputPath, "corrupt.profraw"), selected.object],
    [
      "missing-mapping",
      browserProfile,
      join(outputPath, "missing-mapping.wasm"),
    ],
    [
      "suppressed-transfer",
      join(outputPath, "suppressed.profraw"),
      selected.object,
    ],
  ]) {
    const result = await probe(name, process.execPath, [
      collector,
      "--collect",
      profile,
      mapping,
      join(outputPath, `${name}.dat`),
      profdata,
      cov,
    ]);
    if (result.status === 0) throw new Error(`${name} did not fail collection`);
  }
  if (process.env.COVERAGE_DIR) {
    await mkdir(process.env.COVERAGE_DIR, { recursive: true });
    for (const name of [
      "baseline.dat",
      "wasm.dat",
      "native.dat",
      "browser.profraw",
    ])
      await copyFile(
        join(outputPath, name),
        join(process.env.COVERAGE_DIR, name),
      );
  }
  await writeFile(
    join(outputPath, "summary.json"),
    JSON.stringify(
      {
        selected_mapping: selected.name,
        wasm_hits: counts(selected.lcov),
        native_hits: counts(nativeLcov),
        baseline_hits: counts(baseline),
        tools: toolResults,
      },
      null,
      2,
    ),
  );
  console.log(JSON.stringify(results));
} finally {
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}
