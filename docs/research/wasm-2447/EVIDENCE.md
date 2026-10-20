# Evidence handling

Committed logs contain sanitized technical evidence.
Private originals retain complete logs, raw compiler outputs, and exact generated mapping objects.

The sanitizer replaces private home, cache, sandbox, and temporary roots with `<PRIVATE_ROOT>`.
It replaces remote execution roots with `<EXECROOT>`.
It replaces Nix store hashes with `<NIX_STORE_HASH>` and retains package versions.
It preserves compiler flags, source positions, function names, hashes, counts, exit statuses, and tool versions.
It removes credentials and unrelated process or session identities.

`module-digests.json` records hashes of original artifact bytes.
Sanitized textual copies can have different hashes from their private originals.
Raw `.profraw` evidence contains technical profile data and retains its original bytes.
The Wasm section decoder records structure from both exact module inputs.
The indexed-profile comparison checks compiler function hashes and counter counts for measured subject functions.

The bounded comparison selects `tools/research_wasm/subject.rs`.
Raw exporter output also contains the browser harness records.
Generic filename-only harness baselines do not establish full executable-line coverage for those sources.
No source exclusion or collector exception follows from this experiment.

Additional placeholders identify the checkout (`<CHECKOUT>`), Bazel root (`<BAZEL_ROOT>`), test runfiles (`<RUNFILES>`), and sandbox (`<SANDBOX>`).
`<TMP>` identifies a temporary root. `<REDACTED>` replaces a credential value if one appears.
The committed section text retains section headers. Private originals retain the complete symbol table.
