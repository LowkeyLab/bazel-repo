# Evidence conventions

The public evidence retains technical results from the bounded experiment.
The private receipt directory retains the complete original files.

Raw text artifacts use an additional `.txt` suffix.
This suffix prevents formatters from changing script bytes or coverage positions.
A profile's `textFile` field names the original artifact before that suffix.
The manifest maps public artifacts to their original and sanitized SHA256 values.

The export replaces checkout, artifact, build, Nix-store, temporary-directory, and local-port identities with named placeholders.
The export removes unrelated runtime entries from the server profile.
It retains generated component and Node-helper entries.
The browser export retains script identifiers, ranges, counts, document positions, text, hashes, and map URLs.
It removes unrelated execution-context and stack metadata.
The export removes trailing whitespace from diagnostic lines.
The original profiles remain private.

The `server` directory contains compiler metadata, AST positions, zero coverage, measured coverage, rendered HTML, and Node ranges.
The `browser` directory contains raw Chromium ranges, script text, maps, built HTML, completion receipts, and the interrupted collection error.
The `merge` directory separates canonical original records from unresolved virtual records.
The `controls` directory contains actual collector exits and sanitized diagnostics.
The `checks` directory contains bounded build and test results.

`manifest.json` records effective package pins, executable hashes, source comparisons, action summaries, and control outcomes.
No full BEP file appears here because BEP options can contain credentials.
