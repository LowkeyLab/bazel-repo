# Evidence conventions

The private receipts retain complete original output.
The public artifacts retain sanitized technical evidence.

Raw JSON and HTML artifacts use an additional `.txt` suffix.
This suffix prevents formatters from changing source bytes or positions.
LLVM profiles and rendered HTML use `.hex.txt` to preserve exact original bytes through hexadecimal encoding.
This encoding preserves significant rendered whitespace.
The manifest records the original binary hash and the encoded artifact hash separately.

The export replaces checkout, artifact, build, temporary-directory, and Nix-store identities with named placeholders.
It removes trailing whitespace from diagnostics.
It preserves original template paths, source ranges, function identities, branch outcomes, and execution/cache results.
No full BEP file appears here because options can contain credentials.

The `raw` directory contains native LLVM mappings, an empty-profile baseline, source hashes, and compiler metadata.
It also contains AST metadata, transformed templates, render bytes, events, receipts, controls, and intermediate and combined LCOV.
The `checks` directory contains bounded build, test, formatting, and lint results.
The manifest contains selected BEP action and test summaries.

The expected-record oracle is independent from the emitter's merge-equality assertion.
It requires exactly the three original template identities and one native Rust identity.
It checks exact final template line counters and condition outcomes.
It requires nonempty records and an all-zero parser baseline.
It compares final original input hashes with the test-action hashes.
