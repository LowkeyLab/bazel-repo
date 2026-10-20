import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { once } from "node:events";

const root = process.cwd();
const require = createRequire(resolve(root, "angular/package.json"));
const {
  parse,
  Source,
  visit,
  getLocation,
  print,
  version,
} = require("graphql");
const prefix = "nicknamer2/research/graphql_business/";
const business = "angular/projects/nicknamer2-web/src/app/graphql/";
const mode = process.argv[2];
const output =
  mode === "collect"
    ? process.argv[5]
    : process.env.TEST_UNDECLARED_OUTPUTS_DIR;
mkdirSync(output, { recursive: true });
const save = (name, value) =>
  writeFileSync(
    resolve(output, name),
    typeof value === "string" ? value : JSON.stringify(value, null, 2),
  );
const hash = (value) => createHash("sha256").update(value).digest("hex");
const files = [
  "create-name",
  "create-names",
  "create-server",
  "get-dashboard",
  "get-server-names",
  "get-servers",
];
const expectedLines = {
  "create-name": [1, 2, 3, 4, 5, 6, 7, 8],
  "create-names": [1, 2, 3, 4, 5, 6, 7, 8],
  "create-server": [1, 2, 3, 4, 5, 6, 7, 8, 9],
  "get-dashboard": [1, 2, 3, 4, 5, 6, 7, 8],
  "get-server-names": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 15, 16, 17],
  "get-servers": [1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 13],
  selection: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 16, 17, 18],
  unused: [1, 2, 3],
};
const kinds = [
  "OperationDefinition",
  "Field",
  "FragmentSpread",
  "FragmentDefinition",
  "InlineFragment",
  "Directive",
];
function inventory(key) {
  const path = files.includes(key)
    ? business + key + ".graphql"
    : prefix +
      (key === "unused" ? "disconnected/unused.graphql" : "selection.graphql");
  const input = files.includes(key)
    ? prefix + "documents/" + key + ".graphql"
    : path;
  const text = readFileSync(resolve(root, input), "utf8");
  const source = new Source(text, path);
  const ast = parse(source);
  const nodes = [];
  visit(ast, {
    enter(node) {
      if (kinds.includes(node.kind))
        nodes.push({
          kind: node.kind,
          name:
            node.name?.value ||
            node.typeCondition?.name.value ||
            node.operation,
          offset: node.loc.start,
          ...getLocation(source, node.loc.start),
        });
    },
  });
  assert.deepEqual(
    [...new Set(nodes.map((n) => n.line))].sort((a, b) => a - b),
    expectedLines[key],
  );
  return { key, path, sha256: hash(text), ast, nodes };
}
const selected = [...files, "selection"].map(inventory);
const plain = ({ ast, ...record }) => record;
const id = (path, offset) => `${path}:${offset}`;
function recordCounts(events) {
  const result = {};
  for (const e of events)
    if (
      [
        "dispatch",
        "response-field",
        "fragment-context",
        "inline-context",
        "directive-decision",
      ].includes(e.type)
    ) {
      const key = id(e.path, e.offset);
      result[key] = (result[key] || 0) + 1;
    }
  return result;
}
function expected(records) {
  const result = {};
  for (const doc of records)
    for (const n of doc.nodes) {
      let value = doc.key === "unused" ? 0 : 1;
      if (doc.key === "get-dashboard" && n.kind === "OperationDefinition")
        value = 2;
      if (
        doc.key === "get-server-names" &&
        [7, 8, 9, 10, 11, 12].includes(n.line)
      )
        value = 2;
      if (doc.key === "selection") {
        value = 4;
        if (
          ["FragmentDefinition", "FragmentSpread"].includes(n.kind) ||
          [17, 18].includes(n.line)
        )
          value = 2;
        if (n.kind === "Field" && n.line === 8) value = 2;
      }
      result[id(doc.path, n.offset)] = value;
    }
  return result;
}
function lcov(records, counts, events = []) {
  return (
    records
      .map((doc) => {
        const lines = new Map();
        for (const n of doc.nodes)
          lines.set(
            n.line,
            Math.max(
              lines.get(n.line) || 0,
              counts[id(doc.path, n.offset)] || 0,
            ),
          );
        const rows = [
          "TN:graphql-response-context-research",
          `SF:${doc.path}`,
          ...[...lines]
            .sort((a, b) => a[0] - b[0])
            .map(([line, count]) => `DA:${line},${count}`),
        ];
        for (const n of doc.nodes.filter((n) => n.kind === "Directive"))
          for (const arm of [0, 1])
            rows.push(
              `BRDA:${n.line},${n.offset},${arm},${events.filter((e) => e.type === "directive-decision" && e.path === doc.path && e.offset === n.offset && Number(!e.value) === arm).length}`,
            );
        return rows.join("\n") + "\nend_of_record";
      })
      .join("\n") + "\n"
  );
}
if (mode === "baseline") {
  const records = [...selected, inventory("unused")].map(plain);
  save("baseline.json", { records, node: process.version, graphql: version });
  save("baseline.lcov", lcov(records, {}));
} else if (mode === "execute") {
  assert.equal(
    existsSync(resolve(root, prefix + "disconnected/unused.graphql")),
    false,
  );
  const {
    ApolloClient,
    InMemoryCache,
    HttpLink,
    ApolloLink,
  } = require("@apollo/client/core");
  const bridge = spawn(resolve(root, process.argv[3]), [], {
    stdio: ["pipe", "pipe", "pipe"],
  });
  let stderr = "";
  bridge.stderr.on("data", (data) => {
    stderr += data;
  });
  const lines = createInterface({ input: bridge.stdout })[
    Symbol.asyncIterator
  ]();
  const next = async () => {
    const line = await lines.next();
    assert.equal(line.done, false, stderr);
    return JSON.parse(line.value);
  };
  const ready = await next();
  assert.equal(ready.ready, true);
  assert.equal(ready.memory, 512 * 1024 * 1024);
  assert.equal(ready.nano_cpus, 1_000_000_000);
  save("bridge-ready.json", ready);
  const events = [];
  let current;
  const emit = (type, node, extra = {}) =>
    events.push({ type, path: current.path, offset: node.loc.start, ...extra });
  function materialize(selection, parent, fragments, variables) {
    if (parent === null || parent === undefined) return;
    if (Array.isArray(parent)) {
      for (const item of parent)
        materialize(selection, item, fragments, variables);
      return;
    }
    for (const node of selection.selections) {
      let include = true;
      for (const d of node.directives || []) {
        const arg = d.arguments.find((a) => a.name.value === "if").value;
        const value =
          arg.kind === "Variable" ? variables[arg.name.value] : arg.value;
        emit("directive-decision", d, { value });
        if (
          (d.name.value === "include" && !value) ||
          (d.name.value === "skip" && value)
        )
          include = false;
      }
      if (!include) continue;
      if (node.kind === "Field") {
        const key = node.alias?.value || node.name.value;
        if (Object.hasOwn(parent, key)) {
          emit("response-field", node, { responseKey: key });
          if (node.selectionSet)
            materialize(node.selectionSet, parent[key], fragments, variables);
        }
      } else if (node.kind === "FragmentSpread") {
        const fragment = fragments[node.name.value];
        if (parent.__typename === fragment.typeCondition.name.value) {
          emit("fragment-context", node);
          emit("fragment-context", fragment);
          materialize(fragment.selectionSet, parent, fragments, variables);
        }
      } else if (
        node.kind === "InlineFragment" &&
        (!node.typeCondition ||
          parent.__typename === node.typeCondition.name.value)
      ) {
        emit("inline-context", node);
        materialize(node.selectionSet, parent, fragments, variables);
      }
    }
  }
  const client = new ApolloClient({
    cache: new InMemoryCache(),
    link: ApolloLink.from([
      new ApolloLink((operation, forward) => {
        const definition = current.ast.definitions.find(
          (d) => d.kind === "OperationDefinition",
        );
        emit("dispatch", definition, {
          operationName: operation.operationName,
          variables: operation.variables,
          transmittedDocument: print(operation.query),
        });
        return forward(operation);
      }),
      new HttpLink({
        uri: "http://research/graphql",
        fetch: async (_uri, options) => {
          bridge.stdin.write(options.body + "\n");
          const response = await next();
          events.push({
            type: "router-result",
            path: current.path,
            request: JSON.parse(options.body),
            ...response,
          });
          const definition = current.ast.definitions.find(
            (d) => d.kind === "OperationDefinition",
          );
          const fragments = Object.fromEntries(
            current.ast.definitions
              .filter((d) => d.kind === "FragmentDefinition")
              .map((d) => [d.name.value, d]),
          );
          materialize(
            definition.selectionSet,
            response.body.data,
            fragments,
            JSON.parse(options.body).variables || {},
          );
          return new Response(JSON.stringify(response.body), {
            status: response.status,
            headers: { "content-type": "application/json" },
          });
        },
      }),
    ]),
  });
  async function run(key, variables, check, allowErrors = false) {
    current = selected.find((d) => d.key === key);
    const definition = current.ast.definitions.find(
      (d) => d.kind === "OperationDefinition",
    );
    const result =
      definition.operation === "mutation"
        ? await client.mutate({
            mutation: current.ast,
            variables,
            fetchPolicy: "no-cache",
            errorPolicy: "all",
          })
        : await client.query({
            query: current.ast,
            variables,
            fetchPolicy: "no-cache",
            errorPolicy: "all",
          });
    assert.equal(Boolean(result.error), allowErrors);
    check(result);
  }
  try {
    await run(
      "create-server",
      {
        input: {
          discordServerId: "42",
          displayName: "Research Server",
          clientMutationId: "server",
        },
      },
      (r) => assert.equal(r.data.createServer.server.serverId, "42"),
    );
    await run(
      "create-name",
      {
        input: {
          discordId: "100",
          discordServerId: "42",
          name: "Alpha",
          clientMutationId: "name",
        },
      },
      (r) => assert.equal(r.data.createName.name.name, "Alpha"),
    );
    await run(
      "create-names",
      {
        input: {
          discordServerId: "42",
          names: [{ discordId: "200", name: "Beta" }],
          clientMutationId: "batch",
        },
      },
      (r) => assert.equal(r.data.createNames.names[0].name, "Beta"),
    );
    await run("get-dashboard", { first: 10 }, (r) =>
      assert.equal(r.data.servers.totalCount, 1),
    );
    await run("get-servers", { first: 10 }, (r) =>
      assert.equal(r.data.servers.edges[0].node.displayName, "Research Server"),
    );
    await run("get-server-names", { id: "42", first: 10 }, (r) =>
      assert.deepEqual(
        r.data.server.names.edges.map((e) => e.node.name),
        ["Alpha", "Beta"],
      ),
    );
    await run(
      "get-dashboard",
      { first: 0 },
      (r) => assert.match(r.error.message, /must be at least/),
      true,
    );
    for (const [show, hide] of [
      [false, false],
      [true, false],
      [true, true],
      [false, true],
    ])
      await run("selection", { show, hide }, (r) => {
        const node = r.data.servers.edges[0].node;
        assert.equal("createdAt" in node, show);
        assert.equal("updatedAt" in node, show);
        assert.equal("displayName" in node, !hide);
        assert.equal(node.serverId, "42");
      });
    client.stop();
    bridge.stdin.end();
    const [code] = await once(bridge, "exit");
    assert.equal(code, 0, stderr);
    assert.match(stderr, /"final_names":2/);
    assert.match(stderr, /"final_servers":1/);
    const profile = {
      node: process.version,
      graphql: version,
      records: selected.map(plain),
      events,
    };
    const text = JSON.stringify(profile, null, 2);
    save("observations.json", text);
    save("completion.json", { functional: true, profile_sha256: hash(text) });
    save("bridge.stderr.txt", stderr);
    assert.deepEqual(
      recordCounts(events),
      Object.fromEntries(
        Object.entries(expected(selected)).filter(([, n]) => n > 0),
      ),
    );
    save("functional.json", {
      complete: true,
      businessDocuments: 6,
      selectionCases: 4,
      errorCases: 1,
      requests: 11,
      postgresServers: 1,
      postgresNames: 2,
    });
  } finally {
    client.stop();
    bridge.stdin.end();
    if (bridge.exitCode === null) bridge.kill("SIGTERM");
  }
} else if (mode === "collect") {
  const profile = JSON.parse(readFileSync(process.argv[3], "utf8"));
  const receipt = JSON.parse(readFileSync(process.argv[4], "utf8"));
  const records = [...selected, inventory("unused")].map(plain);
  function validate(value, completion) {
    assert.equal(completion.functional, true);
    assert.equal(
      completion.profile_sha256,
      hash(JSON.stringify(value, null, 2)),
    );
    assert.deepEqual(value.records, selected.map(plain));
    assert.equal(
      value.events.filter((e) => e.type === "router-result").length,
      11,
    );
    const counts = recordCounts(value.events);
    assert.deepEqual(
      counts,
      Object.fromEntries(
        Object.entries(expected(selected)).filter(([, n]) => n > 0),
      ),
    );
    for (const n of selected
      .find((d) => d.key === "selection")
      .nodes.filter((n) => n.kind === "Directive"))
      for (const arm of [true, false])
        assert.equal(
          value.events.filter(
            (e) =>
              e.type === "directive-decision" &&
              e.offset === n.offset &&
              e.value === arm,
          ).length,
          2,
        );
    return counts;
  }
  const measured = validate(profile, receipt);
  const zero = Object.fromEntries(
    Object.keys(expected(records)).map((k) => [k, 0]),
  );
  const add = (left, right) => {
    const result = { ...left };
    for (const [key, value] of Object.entries(right))
      result[key] = (result[key] || 0) + value;
    return result;
  };
  const first = add(zero, measured),
    last = add(measured, zero);
  assert.deepEqual(first, last);
  save("baseline.json", { records });
  save("expected-node-counts.json", expected(records));
  save("measured-node-counts.json", first);
  for (const [name, types] of Object.entries({
    dispatch: ["dispatch"],
    fields: ["response-field"],
    fragments: ["fragment-context", "inline-context"],
    directives: ["directive-decision"],
  })) {
    const subset = profile.events.filter((e) => types.includes(e.type));
    save(`${name}-only.lcov`, lcov(records, recordCounts(subset), subset));
  }
  save("baseline.lcov", lcov(records, zero));
  save("measured.lcov", lcov(selected, measured, profile.events));
  save("combined-zero-first.lcov", lcov(records, first, profile.events));
  save("combined-zero-last.lcov", lcov(records, last, profile.events));
  const faults = {
    noFields: (p) =>
      (p.events = p.events.filter((e) => e.type !== "response-field")),
    noFragments: (p) =>
      (p.events = p.events.filter((e) => e.type !== "fragment-context")),
    noDirectives: (p) =>
      (p.events = p.events.filter((e) => e.type !== "directive-decision")),
    missingResponse: (p) =>
      p.events.splice(
        p.events.findIndex((e) => e.type === "router-result"),
        1,
      ),
    wrongSource: (p) => (p.records[0].sha256 = "wrong"),
  };
  const results = [];
  for (const [name, damage] of Object.entries(faults)) {
    const damaged = structuredClone(profile);
    damage(damaged);
    const completion = {
      functional: true,
      profile_sha256: hash(JSON.stringify(damaged, null, 2)),
    };
    assert.throws(() => validate(damaged, completion));
    save(`fault-${name}.json`, { damaged, completion });
    results.push({ name, rejected: true, functionalStillPassed: true });
  }
  assert.throws(() =>
    validate(profile, {
      functional: false,
      profile_sha256: receipt.profile_sha256,
    }),
  );
  assert.throws(() =>
    validate(profile, { functional: true, profile_sha256: "wrong" }),
  );
  save(
    "controls.json",
    results.concat([
      { name: "incomplete", rejected: true },
      { name: "corruptHash", rejected: true },
    ]),
  );
  save("result.json", {
    boundedResponseContextComplete: true,
    directResolverEntryObserved: false,
    bothOrdersEqual: true,
    documents: records.length,
    semantics:
      "Operation dispatch, response field presence, response-context fragment traversal, and client directive decisions. These are not resolver-entry counters.",
  });
} else throw new Error("Unknown mode");
