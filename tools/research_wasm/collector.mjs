import { spawnSync } from "node:child_process";
import { statSync, writeFileSync } from "node:fs";

export function command(executable, args, env = process.env) {
  const result = spawnSync(executable, args, { encoding: "utf8", env });
  return {
    status: result.status,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
    error: result.error?.message,
  };
}

export function requireCommand(executable, args, env) {
  const result = command(executable, args, env);
  if (result.status !== 0) throw new Error(JSON.stringify(result));
  return result.stdout;
}

export function collect(profile, mapping, output, profdata, cov) {
  if (statSync(profile).size === 0)
    throw new Error("Expected a nonempty profile");
  if (statSync(mapping).size === 0)
    throw new Error("Expected a nonempty mapping object");
  requireCommand(profdata, [
    "merge",
    "--sparse",
    profile,
    "-o",
    `${output}.profdata`,
  ]);
  writeFileSync(
    output,
    requireCommand(cov, [
      "export",
      "-format=lcov",
      `-instr-profile=${output}.profdata`,
      mapping,
    ]),
  );
}

export function canonical(lcov) {
  const records = lcov
    .split("end_of_record")
    .filter((record) =>
      /^SF:.*tools\/research_wasm\/subject\.rs$/m.test(record),
    );
  if (records.length !== 1)
    throw new Error(`Expected one subject record, got ${records.length}`);
  return (
    records[0].trim().replace(/^SF:.*$/m, "SF:tools/research_wasm/subject.rs") +
    "\nend_of_record\n"
  );
}

export function counts(lcov) {
  return Object.fromEntries(
    [...lcov.matchAll(/^DA:(\d+),(\d+)/gm)].map((match) => [
      match[1],
      Number(match[2]),
    ]),
  );
}

if (process.argv[2] === "--collect") {
  collect(...process.argv.slice(3));
}

export function wasmSections(bytes) {
  function integer(state) {
    let value = 0;
    let shift = 0;
    let byte;
    do {
      byte = bytes[state.offset++];
      value |= (byte & 127) << shift;
      shift += 7;
    } while (byte & 128);
    return value >>> 0;
  }
  function string(state) {
    const length = integer(state);
    const value = bytes.toString("utf8", state.offset, state.offset + length);
    state.offset += length;
    return value;
  }
  const state = { offset: 8 };
  const sections = [];
  const dataNames = [];
  while (state.offset < bytes.length) {
    const id = bytes[state.offset++];
    const length = integer(state);
    const end = state.offset + length;
    const section = { id, bytes: length };
    if (id === 0) {
      section.name = string(state);
      if (section.name === "name") {
        while (state.offset < end) {
          const subsection = bytes[state.offset++];
          const size = integer(state);
          const next = state.offset + size;
          if (subsection === 9) {
            const entries = integer(state);
            for (let index = 0; index < entries; index++)
              dataNames.push({ index: integer(state), name: string(state) });
          }
          state.offset = next;
        }
      }
    }
    sections.push(section);
    state.offset = end;
  }
  return { sections, data_segment_names: dataNames };
}
