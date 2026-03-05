# AGENTS.md (Angular Workspace)

This guide covers Angular development within this Bazel monorepo. For repo-wide prerequisites and common commands, see the root `AGENTS.md`.

## Workspace & Commands

- Install NPM deps:

  ```bash
  pnpm install
  ```

- Run Angular CLI via Bazel:

  ```bash
  bazel run //tools:ng -- <args>
  # examples
  bazel run //tools:ng -- version
  bazel run //tools:ng -- generate component feature/example --project mindreadr
  bazel run //tools:ng -- serve mindreadr
  ```

## Project Guides

- Mindreadr: see `angular/projects/mindreadr/AGENTS.md`

Note: Additional Angular projects live under `angular/projects/<name>/` and should include their own `AGENTS.md`.

## Best Practices — TypeScript

- Use strict type checking
- Prefer type inference when the type is obvious
- Avoid `any`; use `unknown` when uncertain

## Best Practices — Angular

- Use standalone components (no NgModules)
- Do not set `standalone: true` in decorators (default is standalone)
- Prefer signals for component state; use `computed()` for derived values
- Lazy load feature routes
- Avoid `@HostBinding`/`@HostListener`; use `host` in decorators instead
- Use `NgOptimizedImage` for static images (not for inline base64)

## Components

- Keep components small and focused
- Use `input()`/`output()` helpers instead of decorators
- Set `changeDetection: ChangeDetectionStrategy.OnPush`
- Prefer inline templates for small components
- Prefer Reactive forms over template-driven
- Avoid `ngClass`/`ngStyle`; use `class`/`style` bindings

## State Management

- Use signals for local state; `set`/`update` (not `mutate`)
- Keep transformations pure and predictable

## Templates

- Keep templates simple; avoid complex logic
- Use native control flow (`@if`, `@for`, `@switch`) over structural directives
- Use `async` pipe for observables

## Services

- Single-responsibility services
- Use `providedIn: 'root'` for singletons
- Prefer `inject()` over constructor injection
