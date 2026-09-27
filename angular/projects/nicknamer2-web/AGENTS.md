# nicknamer2-web

## Commands

```bash
# Build
aspect build //angular/projects/nicknamer2-web

# Dev server (port 4200, hot reload)
bazel run //angular/projects/nicknamer2-web:nicknamer2-web.serve

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
- **Pagination cache policy**: Use `relayStylePagination()` from `@apollo/client/utilities` — do NOT hand-roll merge functions (causes duplication bugs with `cache-and-network` fetch policy)
- **Auth**: Casdoor OAuth2 PKCE via `casdoor-js-sdk`; JWT in `sessionStorage`; `authInterceptor` adds Bearer token
- **Styling**: Tailwind CSS v4 + DaisyUI; inline templates with utility classes; no component-level CSS files; `process_styles` Bazel macro for PostCSS
- **Testing**: Vitest (not Karma); `ApolloTestingModule` + `ApolloTestingController` for GraphQL mocking; `data-testid` for DOM queries; `fixture.componentRef.setInput()` for signal inputs; `apolloController.verify()` in `afterEach`
- **Dev server**: `bazel run //angular/projects/nicknamer2-web:nicknamer2-web.serve` runs `ng serve` on port 4200 with hot reload
- **Functional guard testing**: `CanActivateFn` guards must be called with `{} as any, {} as any` args inside `TestBed.runInInjectionContext()` — TypeScript enforces the type signature even if the guard ignores params

## E2E Testing with agent-browser

After UI changes, run an E2E smoke test using `agent-browser`. The CLI is provided by the Nix dev shell via `github:numtide/llm-agents.nix`; on Linux, that wrapper points at the shell-provided Chromium automatically. Requires the full stack running:

```bash
# 1. Start infrastructure
docker compose -f nicknamer2/docker-compose.yml up -d

# 2. Start backend (from repo root)
DB_URL=postgres://nicknamer2:nicknamer2@localhost:5433/nicknamer2 \
  CASDOOR_CLIENT_ID=nicknamer2-local-dev \
  bazel run //nicknamer2/src/bin:nicknamer2

# 3. Start frontend dev server
bazel run //angular/projects/nicknamer2-web:nicknamer2-web.serve

# 4. Get a JWT via Casdoor password grant
TOKEN=$(curl -s 'http://localhost:8000/api/login/oauth/access_token' \
  -H 'Content-Type: application/x-www-form-urlencoded' \
  -d 'grant_type=password&client_id=nicknamer2-local-dev&client_secret=nicknamer2-local-secret&username=testuser&password=testpass123&scope=profile' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['access_token'])")

# 5. Inject token and test
agent-browser open http://localhost:4200
agent-browser eval "sessionStorage.setItem('casdoor_access_token', '${TOKEN}')"
agent-browser open http://localhost:4200/servers
agent-browser wait --load networkidle
agent-browser snapshot -i  # verify UI state
```

Use `agent-browser snapshot -i` after each navigation to verify element counts and content. Close with `agent-browser close` when done.
