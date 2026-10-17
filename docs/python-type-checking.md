# Python type checking

Run the normal repository lint command from the development shell:

```sh
nix develop --command aspect lint //...
```

The recursive lint configuration runs `ty` and Ruff on first-party Python
libraries, binaries, tests, fixtures, and scripts, including hidden packages.
Gazelle generates their targets. New targets use the same aspects automatically.
The existing command covers future targets without a separate source inventory,
coverage guard, or additional CI job.

## Policy and tooling

Root `pyproject.toml` configures `ty`. Root `.ruff.toml` retains the existing Ruff
rules and enables ANN001, ANN002, ANN003, ANN201, ANN202, ANN204, ANN205, ANN206,
and ANN401. Annotate parameters and returns, including private and nested
functions, tests, and fixtures. Use `-> None` for functions without a result.
Local variables can use inference. Supply generic arguments such as `list[str]`.

The pinned `rules_lint` `multitool` lock provisions native `ty` 0.0.85 and Ruff 0.16.10.
The repository uses that existing hub. Nix builds the Aspect command-line tool, not the checker.
The production `rules_lint` patch includes each target's own imports, dependency
imports and transitive sources. Wheel directory inputs retain their typing
metadata and installed stubs. Both aspects read the Bazel Python selector: `ty`
receives `--python-version 3.15`, and Ruff receives `--target-version py315` for
the current selector. Neither derives the version from the project's minimum.

The Nix-delivered Aspect package embeds the Python policy patch before extracting
its `builtins`. `ty` findings and nonzero checker exits fail lint under every
strategy. Ruff annotation findings also fail regardless of baseline or soft
strategy. Ruff configuration and runtime errors fail the Bazel action. Other
Ruff diagnostics and other linters retain the chosen strategy.
Human diagnostics and machine reports in the Static Analysis Results Interchange
Format remain available through normal lint outputs. The report converter preserves `ty` warnings as well as errors.

## Dynamic boundaries and exceptions

Prefer concrete types, typed dependencies, and small validated adapters. A
suppression must name its rule and explain both the limitation and why a typed
solution is impractical. Keep it on the smallest possible expression. Blanket
file/test exemptions, import ignores, and broad baselines aren't accepted.
Use coded `ty: ignore[...]` comments rather than comments without rule codes or Python
`type: ignore` comments. Explanation quality still requires review.

The CI Nix helper uses a precise `TypedDict` for the documented Nix path-info
JSON schema. Its narrow JSON casts bridge the standard library's dynamic
`json.loads` return. They don't validate malformed provider schemas at runtime.
The acceptance fixture similarly types its own recorded artifacts and verifies
their values through existing behavior tests. Review these external and artifact
boundaries when changing the underlying schema.

Enforcement has limits. `ty` checks supported dynamic flows into statically typed
variable assignments, returns, and yields. It doesn't reject every Any/Unknown
flow. Attribute/subscript destinations and destinations containing dynamic types
weaken this protection. Ruff ANN401 rejects explicit Any in function annotations
but has alias limitations and doesn't prohibit every inferred or variable Any.
Review these remaining boundaries. Don't describe this configuration as a
complete prohibition on dynamic typing.

## Upgrades and validation

Keep automatic stable Python updates. Changing the selector changes both checker
arguments and their action keys. An interpreter, checker, or dependency update
must pass compatibility checks, Python tests, full formatting, full build, and
complete lint before merging. Unsupported language targets or diagnostics must
fail validation. Don't fall back to an older minor or turn off policy to pass.
When changing `ty`, reconcile the explicitly promoted warning rules with the new
release and verify the configuration with the delivered binary.

Acceptance includes clean and deliberately broken targets, soft/baseline
strategies, typed imports and stubs, configuration errors, input invalidation,
and temporary Gazelle-generated targets in ordinary and hidden packages.
Restore temporary probes afterward. Distinguish locally executed checks,
configured CI remote execution, and hosted CI on the final PR head.

See the accepted specification in
[issue #2339](https://github.com/LowkeyLab/bazel-repo/issues/2339) and its
[policy resolution](https://github.com/LowkeyLab/bazel-repo/issues/2323#issuecomment-6232007070).
