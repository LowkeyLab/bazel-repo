# Coding standards

Read these rules before code changes or reviews.
Make only changes that the task requires.
Use environment variables for secrets and credentials. Never hardcode them.
Understand the applicable project structure before you create directories.

## Language standards

These references own the language-specific rules.
Read each reference that applies to the changed files.
Review judgment rules as well as mechanical rules that tools do not enforce.

- Rust: [Rust standards](docs/agents/rust.md).
- Kotlin: [Kotlin standards](docs/agents/kotlin.md).
- Go: [Go standards](docs/agents/go.md).
- TypeScript and Angular: [TypeScript standards](docs/agents/typescript.md).

The [development workflow](docs/agents/development.md) owns commands, dependency selection, and `BUILD`-file procedures.
The [verification inventory](docs/agents/checks.md) distinguishes configured tools from rules that require review.
