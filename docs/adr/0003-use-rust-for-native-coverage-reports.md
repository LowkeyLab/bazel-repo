---
status: accepted
---

# Use Rust for native coverage reports

The maintainer approved full Rust replacement of the three coverage report commands in
[specification #2632](https://github.com/LowkeyLab/bazel-repo/issues/2632).
One library contains the shared build-event module and the language adapters.
The existing Bazel targets and command arguments remain the caller interface.

The shared module owns event decoding, event selection, named-set traversal,
artifact locations, and artifact checks. Adapters select explicit artifact policies.
They consume typed event results. Preserve each family's identity,
ordering, path, integrity, and failure rules. Keep each family's rules distinct.

The adapters retain collector validation, source mapping, evidence, and report generation.
Continue to use the existing Java analyzer, JavaScript collectors, and Bazel coverage merger.
Use maintained parsers when XML or archives require direct parsing.
Keep Python bindings and process protocols outside this design.

Preserve source identities, measured counts, source limits, receipt fields, filenames,
exit statuses, and diagnostics that callers use. Preserve lexical path handling.
Keep unknown coverage distinct from measured zero coverage.
Evidence and report replacement retain their existing separate lifecycle.

The report command remains the primary test interface.
Retain the Python command tests against the Rust binaries.
Add `googletest` tests at the shared build-event interface.
Compare successful and failing fixtures before removing each Python implementation.
Record semantic reports, receipts, exit statuses, and report preservation.
Normalize only JSON formatting and temporary paths.

Implement the ordered tickets #2633, #2634, #2635, and #2636.
The first ticket replaces the Go/Rust command and introduces the shared library.
The later tickets replace the JVM and JavaScript commands, then complete local membership acceptance.
Keep command tests, local membership, configured execution, and hosted evidence separate.
The language membership suites require execution outside the hosted workflow.
Remote verification requires explicit authorization for source and artifact transfer.
