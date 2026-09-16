# Testcontainer Images Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make testcontainer image contents explicit, digest-pinned Bazel inputs.

**Architecture:** MODULE.bazel owns four immutable pulls. Shared image targets
produce Docker archives and digest-derived reference files. Rust and Go fixture
helpers import those archives before Testcontainers starts a container.

**Tech Stack:** rules_oci 2.3.0, Rust testcontainers 0.27.3, Go testcontainers
0.44.0, Bazel runfiles, Docker.

## Tasks

- [x] Establish the baseline with `bazel run //:gazelle` and query existing image
      dependencies. Existing test targets currently have no PostgreSQL OCI inputs.
- [x] Add `test_postgres_11`, `test_postgres_16`, `test_postgres_18`, and
      `test_ryuk` pulls to `MODULE.bazel`, preserving current tags by resolving their
      registry manifest digests. Use linux/amd64, matching existing OCI platforms.
- [x] Create `tools/test_images/defs.bzl` and `BUILD.bazel`. Generate references
      from each pull's `:digest`, use them in `oci_load(repo_tags = ...)`, and expose
      `filegroup(output_group = "tarball")`. Provide shared data/env helpers for
      POSTGRES_IMAGE_TAR, POSTGRES_IMAGE_REF, RYUK_IMAGE_TAR, and RYUK_IMAGE_REF.
- [x] Create `tools/test_images/rust/lib.rs`: use runfiles to resolve the required
      PostgreSQL archive/reference inputs, load with Docker, and cache the resulting
      reference with OnceLock. Return a Postgres request with its name/tag overridden.
      Missing inputs and Docker failures must produce actionable errors.
- [x] Replace Postgres::default starts in Nicknamer's common fixture,
      Nicknamer2's graphql, migrations, name, and discord_server tests, and Prediction
      Bot's store fixture with the shared helper. Run Gazelle immediately after
      editing sources. Declare matching image data/env on their rust_test targets.
- [x] Add image-loading support to `predix/internal/testutil`, resolving runfiles
      and using sync.Once. Set the digest-specific Ryuk Hub prefix before any
      Testcontainers call, load both archives, and pass PostgreSQL's generated
      reference to postgres.Run. Propagate image inputs to every consuming go_test.
      Run Gazelle immediately after source edits.
- [x] Run `aspect test //nicknamer/server/lib/tests:tests
//nicknamer2/src/... //prediction_bot/lib:store_test //predix/internal/...`.
      Confirm real PostgreSQL startup with the Bazel-loaded images and no fallback
      references in fixture code.
- [x] Query the tests' transitive dependencies for the four external image
      repositories. Rerun Gazelle and confirm manual data/env wiring survives.
- [x] Run `aspect format --scope=all`, `aspect build //...`, and `git diff --check`.
      Review the final diff for unrelated generated changes. Record exact outcomes
      and any environmental limitations without claiming unrun checks passed.

## Verification results

All 28 selected Bazel test targets passed (17 integration targets executed,
11 other targets cached). The full `aspect build //...` passed. Bazel query
confirmed paths from the tests through archive targets to all four OCI pulls.
Gazelle left BUILD/Starlark files unchanged on the final run. Independent review
identified a Docker CLI context mismatch, fixed by streaming imports through
Testcontainers' configured clients; the follow-up review found no further issues.
