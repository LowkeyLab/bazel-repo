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
│   └── styles.css          # Global styles with Tailwind directives
├── BUILD.bazel             # Bazel build configuration
├── tsconfig.app.json       # TypeScript config for app
└── tsconfig.spec.json      # TypeScript config for tests
```

## TailwindCSS v4 Configuration

This project uses TailwindCSS v4 with the `@tailwindcss/postcss` plugin. The configuration is minimal:

### PostCSS Configuration

The PostCSS configuration is located at `angular/.postcssrc.json`:

```json
{
  "plugins": {
    "@tailwindcss/postcss": {}
  }
}
```

### CSS Configuration

In `src/styles.css`, we use the `@tailwind` directives:

```css
@tailwind base;
@tailwind components;
@tailwind utilities;
```

## Building

###With Angular CLI (Recommended)

The Angular CLI properly processes TailwindCSS with PostCSS:

```bash
cd angular
ng build tailwind-sample
```

The output will be in `angular/dist/tailwind-sample`.

### With Bazel

Build the project with Bazel:

```bash
bazel build //angular/projects/tailwind-sample:tailwind-sample
```

The output will be in `bazel-bin/angular/projects/tailwind-sample/dist`.

**Note**: There's currently a known limitation where TailwindCSS v4's automatic content detection may not work optimally in the Bazel sandbox environment. This is being investigated. For now, using the Angular CLI (`ng build`) is the recommended approach for building this project.

## Running Tests

Run the tests with:

```bash
bazel test //angular/projects/tailwind-sample:test
```

Or with Angular CLI:

```bash
cd angular
ng test tailwind-sample
```

## Known Limitations

### Content Detection in Bazel

TailwindCSS v4's automatic content detection relies on PostCSS scanning template files to determine which utility classes to generate. In a Bazel sandbox environment:

- File paths are virtualized during the build process
- The PostCSS plugin may not have access to source files at the expected locations
- This can result in utility classes not being detected and included in the final CSS

**Status**: The Angular CLI build (`ng build`) works perfectly and generates all necessary utility classes. The Bazel build integration needs further investigation to ensure PostCSS processes files correctly in the Bazel sandbox.

### Workarounds for Bazel Builds

If you need to use Bazel builds and encounter missing utility classes:

1. Use the Angular CLI for local development and CI builds
2. Investigate custom PostCSS plugin configuration for Bazel
3. Consider using a pre-built CSS approach with explicit utility safelisting

## Technology Stack

- **Angular**: v20.3
- **TailwindCSS**: v4.1.17
- **PostCSS**: v8.5.3 with @tailwindcss/postcss plugin v4.1.17
- **Build System**: Bazel with rules_angular (primary), Angular CLI (recommended for TailwindCSS)

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

## Development

For local development:

```bash
cd angular
ng serve tailwind-sample
```

This will start the Angular dev server on `http://localhost:4200` with TailwindCSS properly configured and hot-reloading enabled.

## Verification

To verify TailwindCSS is working:

1. Build with Angular CLI: `ng build tailwind-sample`
2. Check the generated CSS size - should be ~2KB (minified) with used utilities
3. Open `dist/tailwind-sample/browser/index.html` in a browser
4. Inspect elements to see Tailwind utility classes applied with generated styles

The sample includes visual examples of:

- Gradient backgrounds
- Responsive grid layouts
- Interactive buttons
- Typography variations
- Color schemes
