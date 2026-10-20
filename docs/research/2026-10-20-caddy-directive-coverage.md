# Caddy directive original-source coverage

Issue: [Determine Caddy directive original-source coverage](https://github.com/LowkeyLab/bazel-repo/issues/2459).
Predecessor: [Classify remaining maintained executable sources for coverage](https://github.com/LowkeyLab/bazel-repo/issues/2444).
The [predecessor report](https://github.com/LowkeyLab/bazel-repo/blob/0dd2c8293fe0375af1377c93996bf848dcd3ff80/docs/research/2026-10-19-remaining-source-coverage.md) supplies the source inventory.

## Result

The packaged Caddy runtime supplies concrete middleware entry events through its intrinsic trace option.
A bounded adapter correlates five unique handler configurations with original Caddyfile lines.
It produces independent zero counters before either request runs.
The controlled request supplies three positive counters.
The untouched response and fallback retain zero counters.
This result supports middleware entry coverage for this fixed fixture.
It does not establish complete original-directive or matcher coverage.
The accepted membership policy cannot silently exclude these unmeasured matcher and block locations.

The later human decision must define the measured event.
Middleware entry occurs before `ServeHTTP` executes.
Entry does not prove successful completion of a handler.
A route match, a matcher outcome, and entry into a concrete handler are different events.
No exception or production mechanism receives approval from this experiment.

## Pins and execution path

The refreshed repository base is `df390e4205234e7ea4d8b09be436739a23de27a2`.
The fixture uses `@multitool//tools/caddy`, the same packaged CLI as the maintained `personal_website/cache_headers_test.go` test.
It adapts a Caddyfile, replaces the listener with a Unix socket, and starts the packaged executable.
The test sends exactly one request per process.
It stops each process before it reads the complete log.
The second process enables intrinsic trace and repeats the same request.
Both responses match exactly, including the empty body.

| Input                       | Effective value                                                                 |
| --------------------------- | ------------------------------------------------------------------------------- |
| Caddy CLI                   | `v2.11.7 h1:yj0Y4fYZGPkSvibBJ1sTWE33xC0fxztVyXEW5iIdUT4=`                       |
| Linux archive SHA-256       | `727b91701a392de6ebc5027509f548bf39979e5216340d0faed8fa5e69c84f8b`              |
| Executed binary SHA-256     | `678ade3bfc088749c81a681adc603333ee0bb023b6a6cfe3c0f58bef8ff854e9`              |
| Binary Go compiler          | `go1.26.8`, from `caddy build-info`                                             |
| Original Caddyfile SHA-256  | `8ccec0139c95971fe3ad04326e9ae50acedbb7949d75a56772d7874270d4eab6`              |
| Native adapted JSON SHA-256 | `2203e729ac7fac32e5b472292b43edd6200f09d5e0760cb899d1085285647d92`              |
| Deployment image reference  | `caddy@sha256:685ae3d7c34d9b5f8caeddc5d14051db7bbd88ff1f5a2fa0c16bd8ca328fb7af` |

The CLI archive URL and digest come from `tools/tools.lock.json`.
The deployment image reference comes from `MODULE.bazel`.
This experiment does not run or inspect that deployment image.
It establishes the packaged CLI test path only.

## Native observations

Native adaptation preserves handler configurations and route structure.
Its JSON does not supply original file and line fields for these handlers.
The diagnostic warning supplies a Caddyfile line for formatting advice.
That diagnostic does not supply a complete source map.
Ordinary access logs show the response, but they do not identify original handler lines.

Caddy's lexer retains token file and line positions in its [pinned source](https://github.com/caddyserver/caddy/blob/v2.11.7/caddyconfig/caddyfile/lexer.go).
Those positions do not appear as a handler map in this experiment's CLI output.
The [route directive](https://caddyserver.com/docs/caddyfile/directives/route) preserves the order of its handlers.
The [handle directive](https://caddyserver.com/docs/caddyfile/directives/handle) groups mutually exclusive handlers.
The fixture tests both structures without changing the maintained routing implementation.

The native response is status `307`, `Location: /done`, `X-Stage: before`, and `X-Order: first`.
The traced response matches all fields and body bytes.
The trace contains five middleware entries: two `subroute` events, two `headers` events, and one `static_response` event.
The provisioned subroute objects omit the raw nested handler and matcher details.
The adapter leaves both subroute events unmeasured.

The [pinned middleware wrapper](https://github.com/caddyserver/caddy/blob/v2.11.7/modules/caddyhttp/routes.go) calls `logTrace` immediately before `ServeHTTP`.
The [trace implementation](https://github.com/caddyserver/caddy/blob/v2.11.7/modules/caddyhttp/server.go) logs the module name and provisioned module configuration.
The [trace option](https://github.com/caddyserver/caddy/blob/v2.11.7/modules/caddyhttp/logging.go) requires debug logging and remains experimental.
Route matching occurs separately from the middleware wrapper.
These events do not expose each matcher outcome or a request identifier.

## Fixed-fixture adapter

`tools/research_caddy/adapter_test.go` creates the baseline before starting either Caddy process.
For each concrete single-line candidate, it removes that line and invokes the same packaged Caddy adapter.
It never executes a deletion variant.
It compares concrete `headers` and `static_response` configurations with the original adapted JSON.
Exactly one unique configuration must disappear.
All remaining concrete configurations must stay equal.
The adapter rejects structural candidate lines, duplicate configurations, and nonunique differences.
It requires all five concrete handlers in this fixture to map.
It does not implement a general Caddyfile parser.

The matcher `header` candidate at line 13 removes no concrete handler configuration.
It remains unmeasured.
Each successful association records its original line, column, directive, exact configuration, and adapted JSON pointer.
The collector matches trace configurations to these unique associations.
It rejects unknown concrete handlers and ambiguous associations.
It preserves subroute events separately from measured concrete handlers.

| Original location    | Source role                 | Baseline DA | Measured DA | Observation                         |
| -------------------- | --------------------------- | ----------- | ----------- | ----------------------------------- |
| 12                   | `path /chosen` matcher      | None        | None        | No matcher event                    |
| 13                   | `header X-Mode yes` matcher | None        | None        | No concrete handler difference      |
| 15                   | `route` block               | None        | None        | No original block correlation       |
| 16:9                 | `header X-Stage before`     | 0           | 1           | Unique concrete entry               |
| 17                   | `handle @chosen` block      | None        | None        | Matcher and block remain unmeasured |
| 18:13                | `header X-Order first`      | 0           | 1           | Unique concrete entry               |
| 19:13                | `redir * /done 307`         | 0           | 1           | Unique concrete entry               |
| 21                   | `handle /untouched` block   | None        | None        | No block event                      |
| 22:13                | `respond "untouched" 200`   | 0           | 0           | Independent untouched handler       |
| 24:9                 | `respond "fallback" 200`    | 0           | 0           | Independent fallback handler        |
| 1, 5, 20, 23, 25, 26 | Structural delimiters       | None        | None        | No concrete handler                 |

The five `DA` locations use `SF:tools/research_caddy/Caddyfile`.
The adapter emits no `BRDA` records.
A positive redirect entry does not establish the false outcome of the untouched matcher.
Short-circuit matcher evaluation and mutually exclusive groups prevent that inference.

## Capability matrix

| Capability                    | Native result                             | Adapter result or missing observation            |
| ----------------------------- | ----------------------------------------- | ------------------------------------------------ |
| Original identities           | Handler JSON has no original line map     | Canonical Caddyfile plus five fixed associations |
| Independent executable zeros  | No native LCOV baseline                   | Five associations before request execution       |
| Ordered handler entry         | Trace supplies module configurations      | Three unique original-line hits                  |
| Untouched route               | No trace entry for its concrete response  | Independent response counter stays zero          |
| Redirect behavior             | Functional status and headers pass        | Entry counter matches the redirect configuration |
| Matcher branches              | No per-matcher outcome event              | Unresolved, no inferred branch records           |
| Structural block lines        | Provisioned subroute output loses details | Unmeasured                                       |
| Duplicate configurations      | Trace has no original identity            | Adapter rejects ambiguity                        |
| Missing or corrupt collection | Functional response alone stays valid     | Child collector returns failure                  |
| Concurrent requests           | Trace event has no request identifier     | Not tested, isolated request only                |
| Valid but incomplete trace    | No expected event inventory               | Remains unresolved                               |

## Collection controls and merging

The completion receipt binds original source, mapping, and trace hashes.
The collector checks those hashes before it consumes trace events.
It parses every JSON line strictly.
Five child collectors fail after both functional assertions pass:

1. The trace file disappears.
2. The mapping baseline disappears.
3. The completion receipt disappears.
4. Corrupt JSON receives an updated matching hash but still fails parsing.
5. A duplicate configuration receives matching hashes but fails unique correlation.

The temporary adapter supplies these checks.
They are not intrinsic Caddy coverage behavior.
A receipt cannot prove that a valid trace contains every expected event.
That completeness gap remains open for a production collector design.

Both merge orders preserve the same five nonempty `DA` records and canonical `SF` identity.
An independent exact fixture oracle requires hits at lines 16, 18, and 19.
It requires zeros at lines 22 and 24.
Merge equality alone is insufficient.

## Reproduction and validation

Run the focused Bazel test:

```bash
nix develop --command bazel --host_jvm_args=-Xmx1536m test \
  //tools/research_caddy:native_test --config=ci-remote --jobs=8 \
  --local_resources=cpu=2 --local_resources=memory=2048 --test_output=errors
```

The target preserves native adaptation, deletion variants, runtime traces, responses, independent baseline, mapping, receipt, controls, and combined LCOV.
The evidence directory contains sanitized copies with a `.txt` suffix.
`<EXECROOT>`, `<OUTPUTS>`, and `<SOCKET>` replace irrelevant runtime paths.
`<TIME>` replaces runtime timestamps.
These replacements do not change original source positions, handler configurations, branch limits, or execution results.
The original hashes describe private raw bytes before sanitization.
The artifact manifest describes the committed sanitized bytes.
The initial redirect setup error remains separate from collector results.
The first fixture omitted the wildcard matcher and reached the fallback.
The fixed directive uses `redir * /done 307`.

The final formatted test passed through configured remote execution.
Its [BuildBuddy invocation](https://app.buildbuddy.io/invocation/3eb36da5-134a-46ba-bf76-2c4b4f688ec9) records `strategy: remote` and `executionWallTime: 0.886s`.
The test result has no cache-hit marker.
The command reports eight remote processes and 1592 action cache hits.
These results separate executed test work from reused build actions.
No local Caddy request ran in this final check.
No hosted CI or Codecov upload ran.

Required Gazelle and full repository formatting passed.
Formatting uses the registered Java 21 runtime because the formatter requires that runtime.
This invocation does not change production toolchain registrations.
The focused [lint invocation](https://app.buildbuddy.io/invocation/72ae8627-5360-4b95-9c3a-1b65ca50b4ac) passed with no findings.
Its diagnostic summary lists only KeepSorted.
That result does not establish a general Go linter audit.
The focused Bazel test also compiled the Go fixture and ran its configured `nogo` action.
No broad application build ran for this research-only fixture.

The final original Caddyfile bytes equal the test artifact bytes.
Their SHA-256 equals the runtime input receipt and the mapping baseline.
The source manifest records the final fixture files.
The artifact manifest records every committed sanitized observation.
The report passes the strict-English structural scan.
The assignment remains active pending publication approval.
