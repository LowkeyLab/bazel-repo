# Topcoat Bazel compatibility experiment

This throwaway experiment supports the Book Smartz minimum viable product planning effort. It exercises the selected build interface before the app contract decision.

The task is [Verify the Topcoat Bazel compatibility gate](https://github.com/LowkeyLab/bazel-repo/issues/2271).

<!-- Proper tracker titles retain their acronyms. -->
<!-- vale Google.Acronyms = NO -->

The map is [Plan the Book Smartz bisection-ranking MVP](https://github.com/LowkeyLab/bazel-repo/issues/2214).
<!-- vale Google.Acronyms = YES -->

The root Rust manifest and lock select published Topcoat 0.10.0 with default features and `icon-iconify`. The existing shared compiler remains Rust 1.99.1. The npm catalog and lock provide Tailwind 4.3.3, Roboto 5.3.0, and Feather JSON 1.2.1. Roboto and Feather are compatibility fixtures. Product typography and icons remain undecided.

## Inputs and package

`//tools/topcoat:defs.bzl` wraps the existing Rust rules. Each caller declares its package identity and macro dependency names. A generated Cargo manifest supplies macro lookup metadata. It doesn't run Cargo. A declared environment file carries its relative manifest directory. Explicit companion targets carry library asset inputs to the package.

Topcoat's runtime embeds asset metadata too. Its dependency annotation sets a relative manifest directory and exports the locked browser runtime input. The annotation deliberately names the generated repository for the pinned release. Another dependency configuration requires revalidation.

The Tailwind action copies declared CSS and scanned sources into its own directory, avoiding output conflicts with Rust's generated-input staging. Its npm tool and dependencies remain declared Bazel inputs. Font bytes and `Iconify` JSON come from locked npm packages. The wrapper stages the macro input directory explicitly.

The asset packager rejects remote declarations and paths outside its staging directory. It matches each declaration to exactly one declared input by its complete relative suffix, stages those bytes, and calls the published public `Bundler`. It never edits the packaged executable. Generation and bundling run with the action network off and have no private asset download cache.

The archive contains `app` and adjacent `assets/`, including the manifest, CSS, WOFF2 font, library text asset, and Topcoat JavaScript. The executable contains the generated icon, which the app renders as inline SVG. Runtime startup uses the packaged asset manifest, not build paths.

## Commands

Run from the assigned checkout:

```bash
nix develop --command bazel run //:gazelle
nix develop --command aspect format --scope=all
nix develop --command aspect build //tools/topcoat/compatibility:package
nix develop --command aspect test //tools/topcoat/compatibility:compatibility_test
nix develop --command aspect build //...
nix develop --command aspect lint
git diff --check
```

For an interactive run, supply `DATABASE_URL` for an isolated disposable PostgreSQL database and `FIXTURE_READER_ID` as a value such as `00000000-0000-4000-8000-000000000001`. Optional `HOST` and `PORT` configure the listener. Then run:

```bash
nix develop --command bazel run //tools/topcoat/compatibility:run
```

The launcher extracts the archive and starts it from another temporary directory. `POST /fixture-login` supplies the controlled fixture identity, creates a session row owned by the app, and issues Topcoat's session cookie. This endpoint deliberately substitutes for an identity provider in the experiment.

| Endpoint                    | Purpose                                                                    |
| --------------------------- | -------------------------------------------------------------------------- |
| `GET /`                     | Protected page using real ranking storage and the independent view library |
| `POST /protected-procedure` | Protected procedure, JSON arguments `[]`                                   |
| `POST /protected-shard`     | Protected shard, JSON body `{"args":[],"signals":{}}`                      |

## Evidence boundaries

The integration suite starts an isolated real PostgreSQL fixture from the repository's declared image. It extracts only the package and checks HTTP responses, delivered CSS/font/script/library bytes, rendered SVG, and session restoration after restarting against the same database. The suite tests missing and malformed credentials separately for the page, procedure, and shard. A separate test checks formatter rejection, formatting, and convergence on ordinary Rust and a Topcoat macro.

The formatter composes the pinned rustfmt and the selected release's view/font grammar formatters for the selected feature profile. `font-fontsource` and mail macros, renamed dependencies, other platforms, live identity providers, live catalog services, browser cookie transport policy, complete expiry/revocation/isolation coverage, deployment, and production readiness are outside this evidence.

The [evidence report](evidence/REPORT.md) records observed outcomes and immutable log pointers. Successful source compilation alone doesn't pass the gate.
