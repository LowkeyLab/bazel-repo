# Installed source acceptance

Run `//worktree_pool/install:acceptance` with the exact successful generic `--archive` and `--report`, plus a new absolute `--output` directory. The launcher supplies pinned Cargo/rustc 1.96 GNU, Git 2.36.0 and zlib 1.3.1 source archives, the inherited CLI runner, and shared version 0.1.0.

The Linux x86-64 native build environment needs a linker/C compiler, `make`, `file`, `ldd`, `xz`, `sh`, and `sleep`. Git's exact pinned compressed bytes are hashed before XZ decoding; all native tar members are checked before extraction. The final installed runtime PATH contains only the privately built Git, shell, and sleep.

The driver copies and validates the exact generic handoff before tools, installs with `cargo install --locked --path`, builds the minimum native Git runtime, and executes the full supplied public CLI test runner from an unrelated directory. `runtime.json` is written only after that real runner succeeds. Helper scope: Bazel test_support children; public CLI commands use Cargo-installed artifact.

The offline command tests use a genuine historical report/archive pair for interoperability and controlled rejection cases; they stop at deliberately missing tools and do not certify a new install. Native policy tests also checksum, decode, and extract the actual pinned Git XZ and zlib gzip archives. Fresh real Cargo packaging, native builds, and installed CLI execution remain separate acceptance gates.
