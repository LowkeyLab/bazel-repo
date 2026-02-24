# Nicknamer2 Web Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Create a new Angular application built with Vite and Analog.js plugin to consume the Nicknamer2 GraphQL API.

**Architecture:** A standard Angular Single Page Application situated in the `angular/` workspace, customized to use the Vite builder from `@analogjs/platform`, Apollo Angular for GraphQL, and Tailwind+DaisyUI for styling.

**Tech Stack:** Angular 21, Analog.js Vite Plugin, Apollo Angular, GraphQL Codegen, Tailwind CSS, DaisyUI, Bazel.

---

### Task 1: Scaffold Angular App in Workspace

**Files:**

- Create: `angular/projects/nicknamer2-web/src/main.ts`
- Create: `angular/projects/nicknamer2-web/src/app/app.component.ts`
- Create: `angular/projects/nicknamer2-web/src/index.html`
- Create: `angular/projects/nicknamer2-web/tsconfig.app.json`

**Step 1: Scaffold the basic files**
Write the minimal Angular bootstrap files manually since we are using a custom Vite setup.

`main.ts`:

```typescript
import { bootstrapApplication } from "@angular/platform-browser";
import { AppComponent } from "./app/app.component";
import { appConfig } from "./app/app.config";

bootstrapApplication(AppComponent, appConfig).catch((err) =>
  console.error(err),
);
```

`app/app.component.ts`:

```typescript
import { Component } from "@angular/core";

@Component({
  selector: "app-root",
  standalone: true,
  template: "<h1>Nicknamer2 Web</h1>",
})
export class AppComponent {}
```

`app/app.config.ts`:

```typescript
import { ApplicationConfig } from "@angular/core";
export const appConfig: ApplicationConfig = { providers: [] };
```

`index.html`:

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Nicknamer2 Web</title>
    <base href="/" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
  </head>
  <body>
    <app-root></app-root>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
```

`tsconfig.app.json`:

```json
{
  "extends": "../../tsconfig.json",
  "compilerOptions": {
    "outDir": "../../out-tsc/app",
    "types": []
  },
  "files": ["src/main.ts"],
  "include": ["src/**/*.d.ts"]
}
```

**Step 2: Commit**

```bash
git add angular/projects/nicknamer2-web/
git commit -m "feat: scaffold basic angular files for nicknamer2-web" --no-gpg-sign
```

---

### Task 2: Configure Vite & Analog.js Plugin

**Files:**

- Create: `angular/projects/nicknamer2-web/vite.config.ts`
- Modify: `angular/angular.json`
- Modify: `pnpm-workspace.yaml` (ensure Analog deps exist in root package.json)

**Step 1: Create vite.config.ts**

```typescript
import { defineConfig } from "vite";
import analog from "@analogjs/platform";

export default defineConfig({
  root: __dirname,
  plugins: [
    analog({
      ssr: false,
      static: false,
    }),
  ],
});
```

**Step 2: Add project to angular.json**
Add the `nicknamer2-web` configuration under `projects` using the `@analogjs/platform:vite` builder.

**Step 3: Install Analog.js dependencies**
Run: `bazel run @pnpm -- --dir $PWD install @analogjs/platform @analogjs/vite-plugin-angular vite --filter angular...`

**Step 4: Commit**

```bash
git add angular/angular.json angular/projects/nicknamer2-web/vite.config.ts angular/package.json
git commit -m "build: configure analog vite plugin for nicknamer2-web" --no-gpg-sign
```

---

### Task 3: Configure Tailwind CSS and DaisyUI

**Files:**

- Create: `angular/projects/nicknamer2-web/tailwind.config.js`
- Create: `angular/projects/nicknamer2-web/src/styles.css`

**Step 1: Setup Tailwind config**

```javascript
/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./projects/nicknamer2-web/src/**/*.{html,ts}"],
  theme: {
    extend: {},
  },
  plugins: [require("daisyui")],
  daisyui: {
    themes: ["light", "dark"],
  },
};
```

**Step 2: Add Tailwind directives to styles.css**

```css
@tailwind base;
@tailwind components;
@tailwind utilities;
```

**Step 3: Update index.html to include styles**
Add `<link rel="stylesheet" href="/src/styles.css">` to `index.html`.

**Step 4: Install daisyui**
Run: `bazel run @pnpm -- --dir $PWD install daisyui -w`

**Step 5: Commit**

```bash
git add angular/projects/nicknamer2-web/tailwind.config.js angular/projects/nicknamer2-web/src/styles.css angular/package.json angular/projects/nicknamer2-web/src/index.html
git commit -m "build: configure tailwind and daisyui" --no-gpg-sign
```

---

### Task 4: Setup Apollo Angular & GraphQL Codegen

**Files:**

- Create: `angular/projects/nicknamer2-web/graphql.config.yml`
- Create: `angular/projects/nicknamer2-web/src/app/graphql.provider.ts`
- Modify: `angular/projects/nicknamer2-web/src/app/app.config.ts`

**Step 1: Install Apollo and GraphQL dependencies**
Run: `bazel run @pnpm -- --dir $PWD install apollo-angular @apollo/client graphql --filter angular...`
Run: `bazel run @pnpm -- --dir $PWD install -D @graphql-codegen/cli @graphql-codegen/typescript @graphql-codegen/typescript-operations @graphql-codegen/typescript-apollo-angular --filter angular...`

**Step 2: Configure Apollo Provider**
`graphql.provider.ts`:

```typescript
import { ApplicationConfig, inject } from "@angular/core";
import { ApolloClientOptions, InMemoryCache } from "@apollo/client/core";
import { Apollo, APOLLO_OPTIONS } from "apollo-angular";
import { HttpLink } from "apollo-angular/http";

const uri = "http://localhost:8000/graphql"; // Assuming nicknamer2 runs here

export function createApollo(httpLink: HttpLink): ApolloClientOptions<any> {
  return {
    link: httpLink.create({ uri }),
    cache: new InMemoryCache({
      typePolicies: {
        Query: {
          fields: {
            servers: {
              keyArgs: false,
              merge(existing, incoming) {
                // Relay pagination merge logic
                let edges = existing ? existing.edges : [];
                if (incoming && incoming.edges) {
                  edges = [...edges, ...incoming.edges];
                }
                return {
                  ...incoming,
                  edges,
                };
              },
            },
          },
        },
      },
    }),
  };
}

export const graphqlProvider: ApplicationConfig["providers"] = [
  Apollo,
  {
    provide: APOLLO_OPTIONS,
    useFactory: createApollo,
    deps: [HttpLink],
  },
];
```

**Step 3: Provide Apollo in App Config**

```typescript
import { ApplicationConfig } from "@angular/core";
import { provideHttpClient } from "@angular/common/http";
import { graphqlProvider } from "./graphql.provider";

export const appConfig: ApplicationConfig = {
  providers: [provideHttpClient(), ...graphqlProvider],
};
```

**Step 4: Create Codegen Config**
`graphql.config.yml`:

```yaml
schema: "http://localhost:8000/graphql"
documents: "projects/nicknamer2-web/src/**/*.graphql"
generates:
  projects/nicknamer2-web/src/generated/graphql.ts:
    plugins:
      - "typescript"
      - "typescript-operations"
      - "typescript-apollo-angular"
```

**Step 5: Commit**

```bash
git add .
git commit -m "feat: setup apollo angular and graphql codegen" --no-gpg-sign
```

---

### Task 5: Configure Bazel Rules

**Files:**

- Create: `angular/projects/nicknamer2-web/BUILD.bazel`

**Step 1: Write BUILD.bazel**
Since we are using Analog/Vite, we can create a simple `js_run_devserver` target.

```starlark
load("@aspect_rules_js//js:defs.bzl", "js_run_devserver")

js_run_devserver(
    name = "dev",
    args = ["projects/nicknamer2-web/vite.config.ts"],
    command = "node_modules/.bin/vite",
    chdir = "$(rootpath //angular)",
    data = [
        "//angular:node_modules",
        "//angular/projects/nicknamer2-web:srcs",
    ],
)

filegroup(
    name = "srcs",
    srcs = glob(["**/*"]),
    visibility = ["//visibility:public"],
)
```

_(Note: Gazelle might overwrite this, so use `# keep` if necessary, or let Gazelle handle the ts_project)._

**Step 2: Commit**

```bash
git add angular/projects/nicknamer2-web/BUILD.bazel
git commit -m "build: add bazel configuration for nicknamer2-web" --no-gpg-sign
```
