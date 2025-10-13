# Angular + Tailwind CSS + Bazel

A Hello World Angular application integrated with Bazel and Tailwind CSS, demonstrating modern Angular development with a fast, cacheable build system.

## Features

- ⚡ **Angular 20** - Modern Angular with standalone components
- 🎨 **Tailwind CSS** - Utility-first CSS framework with automatic tree-shaking
- 🏗️ **Bazel** - Fast, scalable build system with incremental builds
- 📦 **PNPM** - Efficient package management
- 🚀 **esbuild** - Lightning-fast JavaScript bundler

## Prerequisites

- **Bazel** 8.4.2+
- **Node.js** 20.19.5+
- **PNPM** 9+
- **Mise** (for tool version management, optional)

## Quick Start

### 1. Install Dependencies

From the repository root:
```bash
cd angular-ngc
pnpm install
```

### 2. Build the Application

From the repository root:
```bash
bazel build //angular-ngc:app
```

The built files will be in `bazel-bin/angular-ngc/prod/`.

### 3. Run Development Server

From the repository root:
```bash
bazel run //angular-ngc:serve
```

This will build the application and start a development server with history API support.

Then open http://localhost:8080 in your browser.

## Project Structure

```
angular-ngc/
├── BUILD.bazel              # Bazel build configuration
├── build_tailwind.sh        # Tailwind CSS build script
├── package.json             # NPM dependencies
├── pnpm-lock.yaml           # Dependency lockfile
├── tsconfig.json            # TypeScript configuration
├── tailwind.config.js       # Tailwind CSS configuration
├── postcss.config.js        # PostCSS configuration
├── index.html               # Entry HTML file
├── main.ts                  # Application bootstrap file
├── polyfills.ts             # Browser polyfills
├── styles.css               # Global styles with Tailwind directives
└── app/
    ├── app.component.ts     # Main component
    ├── app.component.html   # Component template
    ├── app.component.spec.ts # Unit tests
    ├── app.config.ts        # App configuration
    └── app.module.ts        # App module
```

## Build Targets

The `ng_application` macro creates the following targets:

- **`//angular-ngc:app`** - Default production build (minified, optimized)
- **`//angular-ngc:prod`** - Production build (same as `:app`)
- **`//angular-ngc:dev`** - Development build (unminified, faster builds)
- **`//angular-ngc:test`** - Run unit tests with Karma
- **`//angular-ngc:serve`** - Start development server
- **`//angular-ngc:serve-prod`** - Start production server
- **`//angular-ngc:styles`** - Process Tailwind CSS only

### Running Targets

```bash
# Build production bundle
bazel build //angular-ngc:app

# Build development bundle
bazel build //angular-ngc:dev

# Run tests
bazel test //angular-ngc:test

# Start development server (with history API support)
bazel run //angular-ngc:serve

# Process CSS only
bazel build //angular-ngc:styles
```

## Build Output Structure

After building with `bazel build //angular-ngc:app`, the output in `bazel-bin/angular-ngc/prod/` contains:

```
prod/
├── index.html                  # Entry HTML with injected scripts
├── bundle-prod/
│   └── main.js                # Bundled application code
├── polyfills-bundle.js        # Browser polyfills
└── styles-processed.css       # Processed Tailwind CSS
```

## How It Works

### Angular Application Build

The `ng_application` macro (defined in `/defs.bzl`) orchestrates the build:

1. **TypeScript Compilation** - Compiles Angular components and services
2. **CSS Processing** - Processes Tailwind CSS with tree-shaking
3. **Bundling** - Uses esbuild to create optimized bundles with code splitting
4. **Asset Injection** - Injects scripts and styles into `index.html`
5. **Distribution** - Assembles everything into a ready-to-serve directory

### Tailwind CSS Integration

Tailwind CSS is fully integrated into the Bazel build process:

1. **Source Scanning** - The `build_tailwind.sh` script scans all `.html` and `.ts` files
2. **Dynamic Config** - A temporary config is generated with explicit paths
3. **Processing** - TailwindCSS CLI processes `styles.css` and generates optimized CSS
4. **Tree Shaking** - Only the Tailwind utilities actually used are included

**Benefits:**
- ✅ No manual CSS compilation needed
- ✅ CSS automatically updates when source files change
- ✅ Optimized bundle size (~15KB of CSS vs ~3MB full Tailwind)
- ✅ Integrated with Bazel's incremental builds and caching

### Build Caching

Bazel caches all build artifacts:
- **Initial build**: ~5-6 seconds
- **Incremental builds**: Only rebuilds changed files
- **CSS processing**: ~110ms (cached if sources unchanged)

## Tech Stack

- **Angular**: 20.3.4
- **TypeScript**: 5.8.3
- **Tailwind CSS**: 3.4.18
- **esbuild**: Via aspect_rules_esbuild 0.21.0
- **RxJS**: 7.8.2
- **Zone.js**: 0.15.1

## Bazel Toolchain

This project uses the following Bazel rules and versions:

- **aspect_rules_js**: 2.6.2 - JavaScript/Node.js support
- **aspect_rules_ts**: 3.7.0 - TypeScript compilation
- **aspect_rules_esbuild**: 0.21.0 - Fast bundling with esbuild
- **rules_nodejs**: 6.5.2 - Node.js toolchain management
- **Node.js**: 20.19.5 (configured via rules_nodejs)
- **TypeScript Compiler**: 5.6.3 (Bazel toolchain)

These versions ensure compatibility with Angular 20's requirements (Node.js 20+ and TypeScript 5.8+).

## Development Workflow

### Adding New Components

1. Create component files in `app/` directory
2. Import and register in `app.module.ts` or use standalone components
3. Build: `bazel build //angular-ngc:app`

### Adding Tailwind Classes

1. Add Tailwind utility classes to your HTML/TypeScript files
2. Run `bazel build //angular-ngc:app`
3. The new classes will automatically be included in the generated CSS

No separate compilation step needed!

### Testing

Run tests with:
```bash
bazel test //angular-ngc:test
```

For interactive debugging:
```bash
bazel run //angular-ngc:test.server
```

## Important Notes

### Template Handling with esbuild

When using esbuild to bundle Angular applications, templates must be handled correctly:

**✅ Use inline templates:**
```typescript
@Component({
  selector: 'app-root',
  template: `<div>Your HTML here</div>`,
})
```

**❌ Avoid external templates (for now):**
```typescript
// External templates require additional setup
@Component({
  selector: 'app-root',
  templateUrl: './app.component.html',
})
```

### Import Order Matters

In `main.ts`, imports must be in this order:
```typescript
import 'zone.js';           // Must be first!
import '@angular/compiler'; // Required for JIT compilation
```

This ensures:
1. Zone.js patches browser APIs for change detection
2. JIT compiler is available for runtime template compilation

## Configuration Files

### Root MODULE.bazel

Includes:
- `aspect_rules_js` 2.6.2 - JavaScript/Node.js support
- `aspect_rules_ts` 3.7.0 - TypeScript compilation  
- `aspect_rules_esbuild` 0.21.0 - Fast bundling
- `rules_nodejs` 6.5.2 - Node.js toolchain (v20.19.5)
- NPM dependency management via `pnpm-lock.yaml`

### BUILD.bazel

Key rules:
- `npm_link_all_packages()` - Links NPM dependencies
- `ng_application()` - Custom macro that orchestrates the Angular build
- `genrule()` - Processes Tailwind CSS
- `copy_to_directory()` - Assembles final distribution

## Troubleshooting

### CSS Classes Not Appearing

If Tailwind classes don't appear in the generated CSS:

```bash
# Rebuild from scratch
bazel clean
bazel build //angular-ngc:app

# Verify your file is in the glob pattern
# Files must match: app/**/*.html or app/**/*.ts
```

### Build Errors

```bash
# Re-install dependencies
cd angular-ngc
pnpm install

# Verbose build output
bazel build //angular-ngc:app --verbose_failures

# Check for sandbox issues
bazel build //angular-ngc:app --sandbox_debug
```

### Port Already in Use

If port 8080 is already in use:

```bash
# Find and kill the process
lsof -ti:8080 | xargs kill -9

# Or use a different port
python3 -m http.server 8081
```

## Future Enhancements

- [ ] Add hot module replacement (HMR) for development
- [ ] Implement AOT compilation for smaller bundles
- [ ] Add production optimizations (minification enabled in prod target)
- [ ] Add e2e testing with Playwright
- [ ] Configure source maps for debugging
- [ ] Add CSS minification for production
- [ ] Implement content hashing for cache busting
- [ ] Add CI/CD pipeline configuration

## References

- [Angular Documentation](https://angular.dev)
- [Tailwind CSS Documentation](https://tailwindcss.com/docs)
- [Bazel Documentation](https://bazel.build/docs)
- [aspect_rules_js](https://docs.aspect.build/rulesets/aspect_rules_js/)
- [esbuild Documentation](https://esbuild.github.io/)
