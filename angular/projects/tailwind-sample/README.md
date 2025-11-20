# TailwindCSS Sample Project

This sample project demonstrates using TailwindCSS v4 with Angular and rules_angular in a Bazel monorepo.

## Project Structure

```
angular/projects/tailwind-sample/
├── src/
│   ├── app/
│   │   ├── app.ts          # Main component
│   │   ├── app.html        # Template with Tailwind classes
│   │   ├── app.css         # Component styles
│   │   ├── app.config.ts   # App configuration
│   │   ├── app.routes.ts   # Routing configuration
│   │   └── app.spec.ts     # Component tests
│   ├── index.html          # Main HTML file
│   ├── main.ts             # Application bootstrap
│   └── styles.css          # Global styles with Tailwind CSS (pre-generated)
├── BUILD.bazel             # Bazel build configuration
├── tsconfig.app.json       # TypeScript config for app
└── tsconfig.spec.json      # TypeScript config for tests
```

## TailwindCSS v4 Integration with Bazel

This project uses a **pre-generated** approach for TailwindCSS integration with Bazel:

### Why Pre-generated CSS?

The `rules_angular` Bazel rules don't currently support PostCSS configuration files (`.postcssrc.json`) in the Bazel sandbox environment. While the Angular CLI (`ng build`) properly processes TailwindCSS via PostCSS, the Bazel build doesn't have access to these configuration files during the build process.

### Solution: Pre-generated Tailwind CSS

Instead of using `@tailwind` directives that require PostCSS processing, we generate the Tailwind CSS once using the Tailwind CLI and include it directly in `src/styles.css`.

**To regenerate the Tailwind CSS** (when adding new utility classes or updating templates):

```bash
cd angular
npx @tailwindcss/cli --input projects/tailwind-sample/src/styles.css --output /tmp/tailwind.css
cat /tmp/tailwind.css > projects/tailwind-sample/src/styles.css
```

Or use the provided script:

```bash
cd angular
npm run tailwind:generate
```

The generated CSS includes only the utilities used in the project templates (tree-shaken), making it compact (~2KB minified) while still providing all necessary styles.

## Building

### With Bazel (Recommended for CI/CD)

```bash
bazel build //angular/projects/tailwind-sample:tailwind-sample
```

The output will be in `bazel-bin/angular/projects/tailwind-sample/dist`.

### With Angular CLI (For Development)

```bash
cd angular
ng build tailwind-sample
ng serve tailwind-sample  # Dev server with hot reload
```

## Running Tests

```bash
bazel test //angular/projects/tailwind-sample:test
```

Or with Angular CLI:

```bash
cd angular
ng test tailwind-sample
```

## Technology Stack

- **Angular**: v20.3 with zoneless change detection
- **TailwindCSS**: v4.1.17 (pre-generated CSS approach)
- **Build System**: Bazel with rules_angular
- **Testing**: Jasmine/Karma with zoneless configuration

## Sample Features

The sample application demonstrates:

- Typography utilities (text sizes, weights, colors)
- Color palettes (blues, greens, purples, reds, etc.)
- Spacing and layout utilities (padding, margin, gap)
- Flexbox and Grid layouts
- Borders and rounded corners
- Background gradients
- Styled buttons with hover effects
- Responsive design with breakpoints (md, lg)
- Shadows and transitions

## Updating Tailwind CSS

When you add new Tailwind utility classes to your templates:

1. Add the classes to your HTML/TS files
2. Regenerate the CSS using the Tailwind CLI:
   ```bash
   cd angular
   npx @tailwindcss/cli --input projects/tailwind-sample/src/styles.css --output /tmp/new-styles.css
   cat /tmp/new-styles.css > projects/tailwind-sample/src/styles.css
   ```
3. Commit the updated `styles.css` file

The Tailwind CLI will scan your templates and generate only the CSS for classes that are actually used, keeping the bundle size small.

## Development Workflow

For local development:

```bash
cd angular
ng serve tailwind-sample
```

This starts the Angular dev server on `http://localhost:4200` with hot-reloading enabled.

For Bazel builds in CI/CD:

```bash
bazel build //angular/projects/tailwind-sample:tailwind-sample
bazel test //angular/projects/tailwind-sample:test
```

## Comparison: Angular CLI vs Bazel

| Feature | Angular CLI | Bazel |
|---------|-------------|-------|
| TailwindCSS Processing | ✅ Automatic via PostCSS | ✅ Pre-generated CSS |
| Build Time | Faster for single project | Better for monorepo |
| Caching | Local only | Distributed caching |
| Tree-shaking | ✅ Automatic | ✅ Manual regeneration |
| Hot Reload | ✅ Built-in | ❌ Not applicable |

## Future Improvements

If `rules_angular` adds native PostCSS support in the future, this project can be migrated to use `@tailwind` directives directly:

```css
/* Future approach (not currently working in Bazel) */
@tailwind base;
@tailwind components;
@tailwind utilities;
```

Until then, the pre-generated approach provides a reliable, production-ready solution.
