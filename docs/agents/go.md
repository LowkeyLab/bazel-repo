# Go (Predix, Cowsay)

Read this reference before you edit or review Go code.
Use the [development workflow](development.md) for commands and verification.
These rules are the language-specific standards referenced by [CODING_STANDARDS.md](../../CODING_STANDARDS.md).

**Style:** standard Go conventions with `gofmt`

**Imports:**

- Grouped: stdlib, external, internal
- Use `gofumpt` through the development workflow.

**Types & Naming:**

- `camelCase` for unexported, `PascalCase` for exported
- Use the `-er` suffix for interfaces, such as `Reader` or `Handler`.
- Avoid repeated names, such as `user.UserService`. Prefer `user.Service` in that case.

**Error Handling:**

- Return errors as last return value
- Check errors immediately: `if err != nil`
- Wrap errors with context: `fmt.Errorf("operation failed: %w", err)`

**Context:**

- ALWAYS propagate `context.Context` as first argument for I/O operations
- Use `ctx context.Context` parameter name

**Database:**

- Use `sqlc` for type-safe SQL code generation
- Schema in `predix/internal/sql/schema.sql`
- Run `cd predix && sqlc generate` after schema changes

**Testing:**

- Test files: `*_test.go` in same package
- Test functions: `func TestXxx(t *testing.T)`
- Use table-driven tests for multiple cases
