# Angular

This workspace contains Angular projects built with Bazel (`rules_angular`).

## Development server

To start a local development server for a project, run the `.serve` Bazel target:

```bash
# One-shot dev server
bazel run //angular/projects/<project-name>:<project-name>.serve

# With hot-reload (recommended)
ibazel run //angular/projects/<project-name>:<project-name>.serve
```

Once the server is running, open your browser and navigate to `http://localhost:4200/`. Using `ibazel` will automatically reload whenever you modify any of the source files.

## Code scaffolding

To generate a new component, run:

```bash
bazel run //tools:ng -- generate component component-name --project <project-name>
```

For a complete list of available schematics (such as `components`, `directives`, or `pipes`), run:

```bash
bazel run //tools:ng -- generate --help
```

## Building

To build a project, run:

```bash
aspect build //angular/projects/<project-name>:<project-name>
```

Build artifacts appear under `bazel-bin/angular/projects/<project-name>/`.

## Running unit tests

To execute unit tests with the `aspect` Bazel frontend, run:

```bash
aspect test //angular/projects/<project-name>:test
```

## Additional Resources

For more information on Angular, visit the [Angular documentation](https://angular.dev) page.
