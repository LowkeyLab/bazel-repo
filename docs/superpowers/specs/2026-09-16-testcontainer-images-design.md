# Bazel-owned testcontainer images

Approved design: pull the PostgreSQL images used by Nicknamer, Nicknamer2,
Prediction Bot, and Predix through rules_oci, pinned by digest. Preserve the
existing variants: 11-alpine, 16-alpine, and 18 respectively. Include Predix's
Ryuk 0.14.0 cleanup image.

Expose Docker archives through oci_load's tarball output group. Tests declare
archives and generated image-reference files as data inputs. Derive local tags
from rules_oci's digest output so there is no second digest to maintain. Import
archives into the test's Docker daemon before starting containers; fail if an
input is missing or importing fails. Resolve paths through Bazel runfiles, not
the working directory. Keep database setup and cleanup behavior unchanged.

A shared Rust test helper loads PostgreSQL once per test process and returns a
configured Postgres request. Both language helpers stream imports through the
same configured Docker clients as Testcontainers, including TLS and properties
file settings. Predix's existing Go test helper loads PostgreSQL
and Ryuk once per process. Use a digest-specific Docker Hub prefix for Ryuk,
configured before Testcontainers initializes; PostgreSQL uses an explicit
127.0.0.1 registry name so this prefix does not rewrite it.

Verify that image inputs appear in Bazel's dependency graph, run all affected
integration suites, rerun Gazelle to check stability, format the repository,
and run the full build. Docker remains a runtime requirement. Image fetching
belongs to Bazel repository resolution rather than test execution.
