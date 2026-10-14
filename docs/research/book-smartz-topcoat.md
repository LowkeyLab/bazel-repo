# Topcoat integration facts for the Book Smartz MVP

Research for [Investigate Topcoat's Bazel and production integration](https://github.com/LowkeyLab/bazel-repo/issues/2215), under [Plan the Book Smartz bisection-ranking MVP](https://github.com/LowkeyLab/bazel-repo/issues/2214). Inspected on 2026-10-14. This document supplies facts and experiment requirements; it does not select an implementation dependency or authorize application changes.

## Result

Topcoat offers server-rendered Rust pages, browser procedures and shards, application context, session-token mechanics, and separately packaged assets. Its documented build/development commands use Cargo. The investigated revision requires Rust 1.99, whereas this repository pins Rust 1.96.0. Integration therefore needs an explicit compiler/revision decision and a Bazel experiment before the web application contract can claim reproducible builds. [Upstream manifest][up-manifest], [manual setup][setup], [repository module][repo-module].

The fixed Topcoat choice remains viable to investigate: the latest official release with a declared compiler floor within 1.96 is v0.6.2, whose floor is 1.95. That is a candidate, not a proven build or recommended pin. [Release manifest][v062-manifest].

## Evidence boundary and revisions

- Main investigation: `1e0bff7117bf52691095550726d75685c6855ba5`, workspace version 0.10.0, edition 2024, declared minimum Rust 1.99. The upstream toolchain file says `stable`; that does not override the manifest floor. [Manifest][up-manifest], [toolchain][up-toolchain].
- Released v0.10.0: `3e7bb43948a61dadcdba5a79d99c0f3a75de1abc`, declared minimum Rust 1.98. v0.9.0, v0.8.1, v0.8.0, and v0.7.0 also declare 1.98. [v0.10 manifest][v010-manifest], [v0.9 manifest][v09-manifest], [v0.8.1 manifest][v081-manifest], [v0.8 manifest][v08-manifest], [v0.7 manifest][v07-manifest].
- Candidate v0.6.2: `92a76709d63a1c584aedc164584d7bc42e3f9061`, edition 2024, minimum Rust 1.95. It retains session tokens, module routing/discovery, procedures, shards, assets, and optional Tailwind. Its runtime is still experimental. [Manifest][v062-manifest], [facade features][v062-facade], [sessions][v062-session], [runtime][v062-runtime].
- The current monorepo evidence is `c158ad6f5dd1611bc328eb27784c382078fa236b`: stable toolchain 1.96.0, a separately registered January 2026 nightly, and dependency generation from root Cargo.toml/Cargo.lock through `rules_rs`. The old nightly is not evidence of meeting 1.99. [Module][repo-module].

Using v0.6.2 trades newer runtime capabilities for a lower declared compiler floor. It uses `.procedure(...)` and `.shard(...)` registration; procedures generate a random UUID path during expansion, and its documented expression vocabulary includes floating-point numbers rather than today's integer support. Subsequent releases add integer/list support, configurable procedure/shard paths, shard HTML morphing, page-rerun rewriting, records, and navigation. A v0.6.2 design must use its own API and represent revision/UUID values without assuming the newer runtime vocabulary. No downgrade safety or full transitive minimum-version compatibility is established here. [Older procedure guide][v062-procedure], [older procedure source][v062-procedure-source], [older expressions][v062-expr], [runtime changelog][runtime-changelog].

## Routing and the Bazel compilation boundary

At the investigated main revision, `module_router!()` is a declarative macro using `module_path!()` and inventory discovery. Pages and routes emit inventory submissions; the router filters registrations to its root module and derives paths from module names. It does **not** crawl the source filesystem to find modules. Declared Rust modules still have to compile/link into the application. Runtime `.discover()` collects linked procedures and shards; explicit `.route(...)` registration is also supported. [Module macro][module-macro], [module builder][module-builder], [page generation][page-source], [procedure guide][procedure], [shard guide][shard].

A separate macro issue remains: generated framework paths use `proc_macro_crate::crate_name`, which consults the calling Cargo manifest under `CARGO_MANIFEST_DIR`. When it cannot resolve a dependency, the implementation falls back to standalone crate names. The monorepo root manifest currently has no Topcoat entry and first-party Book Smartz packages have Bazel targets rather than local Cargo manifests. Merely attaching a facade dependency to a Bazel target does not establish that macro resolution will choose `::topcoat`. The experiment must declare/stage a correct manifest or other verified compatibility treatment, expose it as an action input, and check generated paths in both application and library targets. [Path resolution source][paths], [root manifest][repo-cargo], [Book Smartz BUILD][store-build].

Also verify inventory survives the optimized Bazel link, procedures/shards are discoverable, source-location-derived endpoint IDs match the binary's runtime assets, and proc-macro include files are declared. These are integration hypotheses, not observed failures. The upstream tree supplies Cargo manifests and includes Markdown documentation through Rust `include_str!`; no Bazel build was attempted. [Macro source][router-macro], [facade manifest][facade], [procedure guide][procedure].

## Sessions, authentication, and private ranking access

Topcoat supplies 32-byte random session tokens, SHA-256 token hashes, expiry values, and start/stop/refresh/rotate operations. The application owns user authentication, account identity, session records, expiry checking, revocation, and persistence. Default cookies use `__Host-`, Secure, HttpOnly, SameSite=Lax, and `/`; cookie/session support must be registered on the router. Cookie changes must precede response streaming. [Session guide][session], [cookie guide][cookie].

Thus Book Smartz needs an explicit authenticated-account-to-`ReaderId` mapping and session schema/operations. Existing reader UUIDs are stream identities, not authentication; the storage caller enforces access control. Topcoat does not choose an identity provider or supply Book Smartz session tables. [Existing storage contract][store-readme], [approved storage design][store-design], [session guide][session].

Pages/layouts cannot protect independently callable shard endpoints. Procedures also accept requests directly. Authenticate, authorize reader ownership, and validate inputs inside every procedure/shard/command boundary; derive reader identity from the verified session instead of trusting a browser-supplied reader UUID. Browser-restored shard signals are inputs, not authoritative ranking state. [Procedure guide][procedure], [shard guards][shard].

The router's default OriginPolicy rejects unsafe browser requests from untrusted origins, but allows safe methods and requests without Origin/fetch metadata. It is a browser-origin defense, not authentication. Choose state-changing POST routes and test the actual proxy/browser flow; the documented cookie policy needs HTTPS. [Origin source][origin], [session security][session].

## Reusing Rust and PostgreSQL storage

Topcoat application context accepts type-indexed `'static + Any + Send + Sync` values, such as pools. The existing `Store` is cloneable and constructed from an SQLx `PgPool` and observer. Source supports a direct application-context adapter around these existing operations; Topcoat's examples do not require replacing storage with Toasty or another ORM. Actual compilation of `Store` as context remains untested. [App context][app-context], [store source][store-source], [upstream dependencies][up-manifest].

Repository SQLx is 0.9 with PostgreSQL/Tokio/TLS/UUID/chrono/migration support. Upstream declares Tokio 1.51.1 and UUID 1.23; the root requirements start at Tokio 1.47.1 and UUID 1.18.1. These major versions have potentially overlapping ranges, but dependency resolution must update and verify the Bazel-generated closure rather than assume the current lock fits. [Root manifest][repo-cargo], [upstream manifest][up-manifest].

The application must retain existing expected revisions, event IDs, UTC instants, accepted-event replay, and commit-uncertainty recovery. The storage owns no automatic retry, clock, authentication, catalog metadata, or pool lifecycle. Map expected/stale/uncertain outcomes into an explicit browser contract; procedure errors otherwise fail the expression without exposing the error value. [Storage README][store-readme], [procedure errors][procedure].

## Assets, Tailwind, and formatting

The upstream asset CLI builds via Cargo before scanning the binary; its bundling helper also queries Cargo metadata. It cannot serve as a drop-in Bazel packaging command. A public lower-level `topcoat_asset::Bundler` accepts binary bytes and an output directory. A proposed Bazel adapter could use that API, with all local asset sources declared, remote files prefetched as pinned inputs, and generated assets retained until bundling. This is a proposed tooling form, not an existing repository target. [CLI bundler][cli-bundle], [public bundler][bundler].

`asset!` captures `CARGO_CRATE_NAME`, `CARGO_MANIFEST_DIR`, and `file!()`; source-relative and manifest-relative paths are resolved later by the bundler. Action sandbox paths and remapped source locations must still resolve at that later stage. The Tailwind macro captures `OUT_DIR/tailwind.css`, which has the same lifetime/path issue. Bundles are not interchangeable across profiles/builds. [Asset implementation][asset-source], [stylesheet macro][stylesheet], [asset guide][asset].

Runtime browser distribution JavaScript is present in the pinned upstream tree and registered as an asset. Its package scripts describe a separate TypeScript/tsup build; compiling the Rust application does not automatically run that frontend toolchain. A consumer can investigate packaging the pinned distribution rather than rebuilding it; modifying upstream browser sources would require explicit Bazel-managed JS build targets. No npm/Cargo commands were executed. [Runtime source][runtime-source], [distribution JS][runtime-js], [browser manifest][browser-package].

Tailwind's documented default uses a Cargo build script, downloads a standalone CLI, scans source, and writes minified CSS to OUT_DIR. BuildConfig supports a provided executable plus explicit input/output/cwd, so a future Bazel action can supply a checksum-pinned CLI and enumerated sources without action-time network access. That action and a generated stylesheet asset are still missing. Optional Fontsource/icon/UI registry features can require staged data or generated OUT_DIR files; avoid enabling the entire feature set without accounting for those inputs. [Tailwind guide][tailwind], [Fontsource build][font-build], [UI registry build][ui-build], [feature manifest][facade].

Repository formatting currently uses Rust 1.96 rustfmt and Prettier for Markdown. Upstream `topcoat fmt` formats macro bodies and offers checks; `--rustfmt` runs rustfmt before the macro pass. A future Bazel-managed formatter integration should prove both passes converge and check the exact macros used. No formatter target for Topcoat exists in the inspected repository configuration. [Repository formatters][repo-format], [Topcoat formatting guide][fmt].

## Production composition

Topcoat's start function binds HOST/PORT, defaulting to 127.0.0.1:3000. It serves TCP or caller-provided listeners, handles Ctrl+C/SIGTERM, and supports graceful request draining; RouterService defaults to a 30-second shutdown timeout. Packaged deployment must set an appropriate listener, align shutdown timeouts with the platform and PostgreSQL pool shutdown, and terminate HTTPS for the default session cookie. [Serve implementation][serve], [service implementation][service], [session guide][session].

Deploy the executable and its matching asset manifest/files together. AssetBundle::load expects `assets` beside the executable; load_dir permits a chosen location. Missing bundle entries panic during rendering. CDN hosting is optional and still requires the matching catalog manifest. [Asset guide][asset], [bundle loading][bundle].

The proxy decision must preserve browser Host/Origin behavior, configure trusted proxy networks for client-IP resolution, and set an explicit external base URL when absolute links are needed. Header trust is opt-in. Streaming live regions use WebSockets at the investigated revision; verify upgrade forwarding and connection timeouts if the chosen UI uses those features, rather than assuming every MVP page requires them. [Origin policy][origin], [proxy handling][proxy], [router builder][builder], [runtime guide][runtime].

## Next decision and exact experiment

[Choose the Topcoat and Rust toolchain compatibility path](https://github.com/LowkeyLab/bazel-repo/issues/2223) should choose an exact upstream revision and whether to retain, separately provision, or upgrade the Rust toolchain. No selection is made by this research.

After that decision, authorize a disposable Bazel integration experiment with these acceptance points:

1. Resolve the chosen facade/subcrates and macros through repository rules_rs; compile one SSR page, one nested module route, one procedure, and one shard under the chosen toolchain. Verify macro manifest inputs, inventory discovery, debug/optimized builds, and generated endpoint registration.
2. Generate a minimal stylesheet with a declared standalone Tailwind input and CLI; bundle one local asset, the stylesheet, and the pinned runtime script using a Bazel-managed adapter over the public Bundler API. Rebuild from a clean sandbox with network access disabled during actions and execute the resulting binary with only packaged files, outside the checkout.
3. Exercise HTTPS session issuance/revocation and direct unauthenticated procedure/shard calls. Put the existing Store in app context, persist one reader's ranking, restart, and reload. Include a second reader, stale command, and retained IDs/revision/time for an uncertain command outcome. These are boundary checks, not an application prototype.
4. Verify the exact wire types selected for UUIDs, revisions, outcomes, and records; run formatting/checks through Bazel and demonstrate stable macro formatting. If live streaming is chosen, prove proxy WebSocket/stream behavior separately.

These are proposed future targets and tests. This investigation ran no application build, browser interaction, Cargo/npm command, source prototype, or live deployment. Validation of this research artifact is documentation formatting and source-link inspection; it does not establish Topcoat/Bazel runtime compatibility.

[up-manifest]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/Cargo.toml
[up-toolchain]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/rust-toolchain.toml
[setup]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/docs/manual_setup.md
[facade]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat/Cargo.toml
[v010-manifest]: https://github.com/tokio-rs/topcoat/blob/3e7bb43948a61dadcdba5a79d99c0f3a75de1abc/Cargo.toml
[v09-manifest]: https://github.com/tokio-rs/topcoat/blob/96e8f9e0932ea883ced2859d462e9d6d3f52ea59/Cargo.toml
[v081-manifest]: https://github.com/tokio-rs/topcoat/blob/e9a6ee88736e7b7fe31086bded7f826b340a3fed/Cargo.toml
[v08-manifest]: https://github.com/tokio-rs/topcoat/blob/149f0de077f15b7e9f7daad101490a8b54d63a1e/Cargo.toml
[v07-manifest]: https://github.com/tokio-rs/topcoat/blob/8ef6d803bf8bc4e269c4566a959d190a9ae21896/Cargo.toml
[v062-manifest]: https://github.com/tokio-rs/topcoat/blob/92a76709d63a1c584aedc164584d7bc42e3f9061/Cargo.toml
[v062-facade]: https://github.com/tokio-rs/topcoat/blob/92a76709d63a1c584aedc164584d7bc42e3f9061/crates/topcoat/Cargo.toml
[v062-session]: https://github.com/tokio-rs/topcoat/blob/92a76709d63a1c584aedc164584d7bc42e3f9061/crates/topcoat/docs/session.md
[v062-runtime]: https://github.com/tokio-rs/topcoat/blob/92a76709d63a1c584aedc164584d7bc42e3f9061/crates/topcoat/docs/runtime.md
[v062-procedure]: https://github.com/tokio-rs/topcoat/blob/92a76709d63a1c584aedc164584d7bc42e3f9061/crates/topcoat-runtime/macro/docs/procedure.md
[v062-procedure-source]: https://github.com/tokio-rs/topcoat/blob/92a76709d63a1c584aedc164584d7bc42e3f9061/crates/topcoat-runtime/grammar/src/procedure.rs
[v062-expr]: https://github.com/tokio-rs/topcoat/blob/92a76709d63a1c584aedc164584d7bc42e3f9061/crates/topcoat-runtime/macro/docs/expr.md
[runtime-changelog]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-runtime/CHANGELOG.md
[module-macro]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-router/src/module.rs
[module-builder]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-router/src/module/router.rs
[page-source]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-router/grammar/src/page.rs
[paths]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-core/grammar/src/paths.rs
[router-macro]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-router/macro/src/lib.rs
[procedure]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/docs/runtime/procedure.md
[shard]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/docs/runtime/shard.md
[session]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/docs/session.md
[cookie]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/docs/cookie.md
[app-context]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/docs/context/app_context.md
[origin]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-router/src/origin.rs
[cli-bundle]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-cli/src/asset/bundle.rs
[bundler]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-asset/src/bundler.rs
[asset-source]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-asset/src/asset.rs
[asset]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/docs/asset.md
[bundle]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-asset/src/bundle.rs
[stylesheet]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-tailwind/src/stylesheet.rs
[tailwind]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/docs/tailwind.md
[font-build]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-font/build.rs
[ui-build]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-ui/registry/build.rs
[fmt]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/docs/cli/fmt.md
[runtime]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/docs/runtime.md
[runtime-source]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-runtime/src/lib.rs
[runtime-js]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-runtime/browser/dist/index.js
[browser-package]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-runtime/browser/package.json
[serve]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat/src/serve.rs
[service]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-router/src/service.rs
[proxy]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-router/src/proxy.rs
[builder]: https://github.com/tokio-rs/topcoat/blob/1e0bff7117bf52691095550726d75685c6855ba5/crates/topcoat-router/src/builder.rs
[repo-module]: https://github.com/LowkeyLab/bazel-repo/blob/c158ad6f5dd1611bc328eb27784c382078fa236b/MODULE.bazel
[repo-cargo]: https://github.com/LowkeyLab/bazel-repo/blob/c158ad6f5dd1611bc328eb27784c382078fa236b/Cargo.toml
[repo-format]: https://github.com/LowkeyLab/bazel-repo/blob/c158ad6f5dd1611bc328eb27784c382078fa236b/tools/format/BUILD.bazel
[store-readme]: https://github.com/LowkeyLab/bazel-repo/blob/c158ad6f5dd1611bc328eb27784c382078fa236b/book_smartz/storage/README.md
[store-source]: https://github.com/LowkeyLab/bazel-repo/blob/c158ad6f5dd1611bc328eb27784c382078fa236b/book_smartz/storage/lib.rs
[store-build]: https://github.com/LowkeyLab/bazel-repo/blob/c158ad6f5dd1611bc328eb27784c382078fa236b/book_smartz/storage/BUILD.bazel
[store-design]: https://github.com/LowkeyLab/bazel-repo/blob/c158ad6f5dd1611bc328eb27784c382078fa236b/docs/superpowers/specs/2026-10-01-book-smartz-postgres-design.md
