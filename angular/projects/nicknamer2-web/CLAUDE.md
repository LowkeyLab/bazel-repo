# nicknamer2-web

## Commands

```bash
# Build
aspect build //angular/projects/nicknamer2-web

# Test (Vitest + jsdom)
aspect test //angular/projects/nicknamer2-web:test

# Lint
aspect lint //angular/projects/nicknamer2-web/...

# Regenerate GraphQL types (backend must be running)
graphql-codegen --config angular/projects/nicknamer2-web/codegen.ts
```

## Patterns

- **Signals for state**: `signal()` for mutable state, `computed()` for derived, `effect()` for side effects — no external state library
- **Apollo → signals bridge**: Apollo `valueChanges` Observable piped through `.subscribe()` + `takeUntilDestroyed()` into signals
- **GraphQL codegen**: `.graphql` files in `src/app/graphql/` → `codegen.ts` introspects backend → generates typed services in `src/generated/graphql.ts`
- **Relay cursor pagination**: `InMemoryCache` type policies merge paginated results for `servers` and `Server.names`
- **Auth**: Casdoor OAuth2 PKCE via `casdoor-js-sdk`; JWT in `sessionStorage`; `authInterceptor` adds Bearer token
- **Styling**: Tailwind CSS v4 + DaisyUI; inline templates with utility classes; no component-level CSS files; `process_styles` Bazel macro for PostCSS
- **Testing**: Vitest (not Karma); `ApolloTestingModule` + `ApolloTestingController` for GraphQL mocking; `data-testid` for DOM queries; `fixture.componentRef.setInput()` for signal inputs; `apolloController.verify()` in `afterEach`
