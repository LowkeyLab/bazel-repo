# Generated dependency evidence

The repository-root `Cargo.toml` and `Cargo.lock` remain the Bazel dependency
inputs. `//worktree_pool:publish_files` derives a consumer manifest from the
selected configured Bazel library, binary, and test targets. No consumer manifest
or lockfile is maintained in `worktree_pool`.

After generation, inspect
`bazel-bin/worktree_pool/publish_files.package/Cargo.toml`. Its `[dependencies]`
and `[dev-dependencies]` tables record exact requirements, Cargo package names,
crate aliases, default-feature settings, and explicit requested features. The
sibling `publish_files.json` records source hashes, selected targets, and direct
registry provenance. A dependency used by production and selected tests can appear
in both tables. Aliases such as `cloudevents` and `tracing_subscriber` preserve
Rust import names while identifying their registry packages.

Run [explicit package validation](CONTRIBUTING.md#source-package-preparation-and-installed-acceptance)
and inspect the printed report path. `dependencies.requested` preserves the
selected direct declarations; `dependencies.observed` records Cargo's resolved
transitive graph, versions, and feature activation. `dependencies.metadata_path`
points to retained locked Cargo metadata. The packaged `Cargo.lock` records that
standalone resolution. Validation compares declarations, source and asset hashes,
archive contents, and locked builds; it doesn't assume Cargo's transitive feature
activation matches Bazel's shared feature union. The installer retains the exact
validated report as `validation-report.json` beside its artifact and runtime
evidence. A failed or incomplete report isn't a supported installation handoff.

The dependency roles remain useful without maintaining a second version table:

- `clap` defines the CLI; `serde`, `serde_json`, and `config` handle structured
  data and configuration.
- `redb` stores durable catalog events and projections; `cloudevents-sdk` provides
  the event envelope.
- `chrono`, `uuid`, and `sha2` supply timestamps, identities, and content hashes.
- `thiserror`, `tracing`, and `tracing-subscriber` provide errors and diagnostics.
- `googletest`, `insta`, `proptest`, and `tempfile` support selected tests. Shared
  dependencies also remain represented in the generated development table.

App execution needs Git and ordinary native C runtime libraries. Rust,
Cargo, Bazel, Python and Nix are build/verification tools, not app
runtime dependencies. The minimum-Git acceptance build disables optional
HTTP, transport layer security, Perl, Python, Tcl/Tk, `gettext` and `iconv` support,
and links pinned `zlib` 1.3.1 statically. It proves ordinary local Git transport.
It doesn't certify HTTPS/SSH or that custom Git build as a general replacement
for system Git.
