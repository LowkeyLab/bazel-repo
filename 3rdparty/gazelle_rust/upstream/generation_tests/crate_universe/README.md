# Crate-universe fixtures

Each subdirectory is a golden workspace testing dependency resolution. Its
manifests, workspace files, and lockfiles are test data, not standalone supported
builds. Run these fixtures from the monorepo root:

```sh
nix develop --command aspect test //3rdparty/gazelle_rust/upstream/generation_tests/crate_universe/...
```

Preserve the historical lockfile formats and expected rules. Change a fixture only
when deliberately testing a changed contract; do not repin the suite wholesale.
The unsupported unused-crate case remains explicitly manual.
