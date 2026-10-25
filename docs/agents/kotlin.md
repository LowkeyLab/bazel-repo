# Kotlin (Mindreadr)

Read this reference before you edit or review Kotlin code.
Use the [development workflow](development.md) for commands and verification.
These rules are the language-specific standards referenced by [CODING_STANDARDS.md](../../CODING_STANDARDS.md).

<!-- Proper names retain their capitalization after these labels. -->
<!-- vale Google.Colons = NO -->

**Style:** Ktor framework patterns
<!-- vale Google.Colons = YES -->

**Imports:**

- Organized: wildcard (\*), java.**, javax.**, `kotlin.**`, project imports (^)
- Configured in `.editorconfig`

**Formatting:**

- 2-space indentation
- Max line length: 200 characters
- The development workflow selects ktfmt for formatting.

**Types & Naming:**

- Use type inference when obvious
- `camelCase` for functions, variables, properties
- `PascalCase` for classes, interfaces, objects
- Avoid abbreviations

**Async:**

- Use Kotlin Coroutines for async operations
- Prefer `suspend` functions over callbacks

**Testing:**

- JUnit 5 framework
- Test files mirror source structure in `src/test/`
