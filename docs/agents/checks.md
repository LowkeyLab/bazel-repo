# Check inventory

This inventory records the existing checks for the relocated rules.
It does not introduce a new style policy.
The [development workflow](development.md) owns the command sequence.

## Configured checks

| Rule family                        | Existing verification and configuration                                                              | Limit                                                        |
| ---------------------------------- | ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| Markdown layout                    | Prettier in [format targets](../../tools/format/BUILD.bazel)                                         | Does not validate instruction precedence or pointer validity |
| Markdown prose                     | Vale in [lint targets](../../tools/lint/BUILD.bazel), with [configuration for Vale](../../.vale.ini) | Does not certify strict `ASD-STE100` compliance              |
| Rust formatting                    | [Topcoat formatter](../../tools/topcoat/format.rs) through the format target                         | Does not select domain types or module boundaries            |
| Rust naming and suspicious code    | Clippy aspects in [linters.bzl](../../tools/lint/linters.bzl)                                        | Configured warnings do not enforce all judgment rules        |
| Kotlin whitespace and imports      | ktfmt and `ktlint`, with [.editorconfig](../../.editorconfig)                                        | That file turns off some `ktlint` rules                      |
| Go formatting and imports          | gofumpt through the format target                                                                    | Does not enforce all naming, error, or context rules         |
| TypeScript types and common errors | Bazel TypeScript compilation and [ESLint configuration](../../eslint.config.mjs)                     | No Angular-specific lint plugin in this configuration        |
| JavaScript and TypeScript layout   | Prettier, with [prettier.config.mjs](../../prettier.config.mjs)                                      | Does not enforce semantic naming or framework choices        |
| `BUILD` layout                     | `buildifier` through the format target                                                               | Does not enforce when the caller runs Gazelle                |
| Lint selection                     | [Aspect configuration](../../.aspect/config.axl)                                                     | Runs configured aspects against applicable targets           |

The full Markdown baseline test is `//tools/lint:vale_test`.
The root `//:markdown` filegroup includes these references through its Markdown glob.
The existing Angular, Nicknamer, and Mindreadr Markdown groups cover their changed guides.

## Rules that require review

The inspected configuration does not provide complete deterministic enforcement for these requirements:

- The immediate Gazelle sequence, Bazel-only operations, and the required final command sequence.
- Ownership, receipt retention, stopped-caller confirmation, and recovery approval from inherited pool instructions.
- Strict `ASD-STE100` writing, conditional pointer validity, and instruction precedence.
- Minimal scope, directory selection, existing-library preference, and secrets supplied through environment variables.
- Rust import grouping, newtype invariants, library selection, Tokio-only runtime use, tracing, and test-library choices.
- Kotlin naming, type inference, Ktor patterns, coroutines, and test-directory structure.
- Go naming, interface suffixes, error position and wrapping, context propagation, SQL generation, and table-driven tests.
- TypeScript import grouping and absolute paths, circular dependencies, object interfaces, immutability, and naming.
- Angular standalone defaults, signals, injection, change detection, control flow, templates, forms, services, and images.
- `BUILD` `# keep` decisions and dependency selection.

Tools can detect individual cases within these families.
Review the applicable [standards](../../CODING_STANDARDS.md) for the remaining rules.
Do not treat successful verification as complete enforcement of a family.

## Documentation inventory

The previous root guide combined commands, language rules, repository paths, and repeated workflow warnings.
The language rules now reside in separate references.
The Angular and Nicknamer guides retain project operations and reference the common standards.
The Mindreadr guides retain project targets and reference the common workflow.
The issue-tracker, triage-label, and domain references retain their existing entry points.

The previous root also named `GEMINI.md` and `.github/copilot-instructions.md`.
Neither file exists at the implementation base.
The revised root removes those invalid pointers and retains the available project guides.
