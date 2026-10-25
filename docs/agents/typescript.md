# TypeScript / Angular

Read this reference before you edit or review TypeScript or Angular code.
Use the [development workflow](development.md) for commands and verification.
These rules are the language-specific standards referenced by [CODING_STANDARDS.md](../../CODING_STANDARDS.md).

**Strictness:** all projects use `strict: true` in `tsconfig.json`

**Imports:**

- Organize: framework, third-party, internal
- Use absolute imports from `tsconfig.json` paths
- Avoid circular dependencies

**Types:**

- Use type inference when obvious
- Avoid `any`. Use `unknown` if the type is uncertain.
- Prefer interfaces for object shapes
- Use `readonly` for immutable data

**Naming:**

- `camelCase` for functions, variables, properties
- `PascalCase` for classes, interfaces, types
- `SCREAMING_SNAKE_CASE` for constants

**Angular Conventions:**

- Use standalone components. Do not use NgModules.
- Omit `standalone: true`. It is the default.
- Use `inject()` over constructor injection
- Use signals for state: `input()`, `output()`, `signal()`, `computed()`
- Set `changeDetection: ChangeDetectionStrategy.OnPush`
- Use native control flow: `@if`, `@for`, and `@switch`.
- Do not use `*ngIf` or `*ngFor`.
- Use `async` pipe for observables
- Prefer inline templates for small components

**Services:**

- Single responsibility
- Use `providedIn: 'root'` for singletons

**Formatting:**

- Use two-space indentation with Prettier.
- Use the formatting step in the development workflow.

## Angular workspace rules

The following rules also apply under `angular/`:

- Lazy load feature routes.
- Use `host` in decorators instead of `@HostBinding` or `@HostListener`.
- Use `NgOptimizedImage` for static images, except inline base64 images.
- Keep components small and focused.
- Prefer Reactive forms over template-driven forms.
- Use `class` and `style` bindings instead of `ngClass` and `ngStyle`.
- Use `set` and `update` for signals instead of `mutate`.
- Keep state transformations pure and predictable.
- Keep templates simple. Avoid complex template logic.
