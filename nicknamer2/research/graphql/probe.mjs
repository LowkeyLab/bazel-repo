import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";

const root = process.cwd();
const require = createRequire(resolve(root, "angular/package.json"));
const {
  parse,
  Source,
  visit,
  getLocation,
  print,
  separateOperations,
  version,
} = require("graphql");
const prefix = "nicknamer2/research/graphql/";
const output =
  process.env.TEST_UNDECLARED_OUTPUTS_DIR ||
  resolve(root, "graphql-probe-output");
mkdirSync(output, { recursive: true });
const mode = process.argv[2] || "execute";
const save = (name, value) =>
  writeFileSync(
    resolve(output, name),
    typeof value === "string" ? value : JSON.stringify(value, null, 2),
  );
const digest = (text) => createHash("sha256").update(text).digest("hex");
function inventory(path) {
  const text = readFileSync(resolve(root, path), "utf8");
  const source = new Source(text, path);
  const ast = parse(source);
  const locations = [];
  visit(ast, {
    enter(node) {
      if (
        [
          "OperationDefinition",
          "Field",
          "FragmentSpread",
          "FragmentDefinition",
          "InlineFragment",
          "Directive",
        ].includes(node.kind)
      ) {
        locations.push({
          kind: node.kind,
          name: node.name?.value || node.operation,
          offset: node.loc.start,
          ...getLocation(source, node.loc.start),
        });
      }
    },
  });
  return { path, text, sha256: digest(text), ast, locations };
}
function lcov(records) {
  return (
    records
      .map(({ path, locations, hits = {} }) => {
        const lines = [...new Set(locations.map((x) => x.line))].sort(
          (a, b) => a - b,
        );
        return [
          "TN:graphql-research",
          `SF:${path}`,
          ...lines.map((line) => `DA:${line},${hits[line] || 0}`),
          `LF:${lines.length}`,
          `LH:${lines.filter((line) => hits[line] > 0).length}`,
          "end_of_record",
        ].join("\n");
      })
      .join("\n") + "\n"
  );
}
const selected = inventory(`${prefix}selected.graphql`);
const disconnected = `${prefix}disconnected/unused.graphql`;
if (mode === "baseline") {
  const records = [selected, inventory(disconnected)];
  save("baseline.json", {
    node: process.version,
    graphql: version,
    records: records.map(({ ast, text, ...rest }) => rest),
  });
  save("baseline.lcov", lcov(records));
  save("original-selected.graphql", selected.text);
  save("original-disconnected.graphql", records[1].text);
  console.log(
    JSON.stringify({
      mode,
      paths: records.map((x) => x.path),
      model: "Candidate AST start lines. No runtime events.",
    }),
  );
} else {
  assert.equal(
    existsSync(resolve(root, disconnected)),
    false,
    "The disconnected source must stay outside the selected test runfiles.",
  );
  const {
    ApolloClient,
    InMemoryCache,
    HttpLink,
    ApolloLink,
  } = require("@apollo/client/core");
  const operations = separateOperations(selected.ast);
  const events = [];
  const client = new ApolloClient({
    cache: new InMemoryCache(),
    link: ApolloLink.from([
      new ApolloLink((operation, forward) => {
        events.push({
          type: "dispatch",
          operationName: operation.operationName,
          variables: operation.variables,
          transmittedDocument: print(operation.query),
        });
        return forward(operation);
      }),
      new HttpLink({
        uri: "http://research/graphql",
        fetch: async (_uri, options) => {
          const child = spawnSync(resolve(root, process.argv[3]), [], {
            input: options.body,
            encoding: "utf8",
          });
          assert.equal(child.status, 0, child.stderr);
          const observation = JSON.parse(child.stdout);
          assert.equal(observation.status, 200);
          assert.equal(observation.body.errors, undefined);
          events.push({
            type: "router-result",
            request: JSON.parse(options.body),
            exitStatus: child.status,
            stderr: child.stderr,
            ...observation,
          });
          return new Response(JSON.stringify(observation.body), {
            status: observation.status,
            headers: { "content-type": "application/json" },
          });
        },
      }),
    ]),
  });
  for (const show of [false, true]) {
    const result = await client.query({
      query: operations.CoverageProbe,
      variables: { show },
      fetchPolicy: "no-cache",
    });
    assert.equal(result.data.__schema.queryType.name, "QueryRoot");
    assert.equal("kind" in result.data.__schema.queryType, show);
  }
  const mutation = await client.mutate({
    mutation: operations.CoverageMutation,
    fetchPolicy: "no-cache",
  });
  assert.equal(mutation.data.__typename, "MutationRoot");
  client.stop();
  const receipt = {
    document: selected.path,
    sha256: selected.sha256,
    node: process.version,
    graphql: version,
    locations: selected.locations,
    events,
  };
  save("observations.json", receipt);
  save("original-selected.graphql", selected.text);
  const hits = {};
  for (const event of events.filter((x) => x.type === "dispatch")) {
    const location = selected.locations.find(
      (x) => x.kind === "OperationDefinition" && x.name === event.operationName,
    );
    assert.ok(location);
    hits[location.line] = (hits[location.line] || 0) + 1;
  }
  // Only operation-start lines receive measured dispatch hits.
  // Other candidate lines remain unmeasured. Their zeros are baseline placeholders.
  save("dispatch-only.lcov", lcov([{ ...selected, hits }]));
  const validate = (text) => {
    assert.ok(text, "Collection output is missing.");
    const parsed = JSON.parse(text);
    assert.equal(parsed.document, selected.path);
    assert.equal(parsed.sha256, selected.sha256);
    assert.deepEqual(parsed.locations, selected.locations);
    assert.equal(parsed.events.filter((x) => x.type === "dispatch").length, 3);
    assert.equal(
      parsed.events.filter((x) => x.type === "router-result").length,
      3,
    );
    for (const event of parsed.events.filter(
      (x) => x.type === "router-result",
    )) {
      assert.equal(event.status, 200);
      assert.equal(event.body.errors, undefined);
    }
  };
  validate(JSON.stringify(receipt));
  const faults = {
    missing: undefined,
    empty: "",
    corrupt: "{broken",
    noEvents: JSON.stringify({ ...receipt, events: [] }),
    partial: JSON.stringify({ ...receipt, events: events.slice(0, -1) }),
    wrongSource: JSON.stringify({ ...receipt, sha256: "wrong" }),
    wrongLocations: JSON.stringify({ ...receipt, locations: [] }),
  };
  for (const [name, value] of Object.entries(faults))
    assert.throws(() => validate(value), undefined, name);
  save("capabilities.json", {
    mode,
    node: process.version,
    graphql: version,
    queryDispatches: 2,
    mutationDispatches: 1,
    routerResponses: 3,
    directiveFalse: true,
    directiveTrue: true,
    disconnectedAbsent: true,
    faultChecks: Object.keys(faults),
    serverSelectionEvents: false,
    functionalResultsPreservedBeforeFaultChecks: true,
  });
  console.log(readFileSync(resolve(output, "capabilities.json"), "utf8"));
}
