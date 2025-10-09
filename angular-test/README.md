# Angular + Tailwind CSS + Bazel

A Hello World Angular application integrated with Bazel and Tailwind CSS.

## Project Structure

```
angular-test/
├── BUILD.bazel           # Bazel build configuration
├── build_tailwind.sh    # Tailwind CSS build script
├── package.json          # NPM dependencies
├── pnpm-lock.yaml       # Lockfile for dependencies
├── tsconfig.json        # TypeScript configuration
├── tailwind.config.js   # Tailwind CSS configuration
├── postcss.config.js    # PostCSS configuration
└── src/
    ├── index.html       # Entry HTML file
    ├── main.ts          # Bootstrap file
    ├── styles.css       # Global styles with Tailwind directives
    └── app/
        ├── app.component.ts    # Main component
        ├── app.component.html  # Component template
        └── app.config.ts       # App configuration
```

## Features

- ⚡ **Angular 19** - Modern Angular with standalone components
- 🎨 **Tailwind CSS** - Utility-first CSS framework
- 🏗️ **Bazel** - Fast, scalable build system
- 📦 **PNPM** - Efficient package management

## Prerequisites

- Bazel 8.4.2+
- Node.js 20.18.0
- PNPM 9+
- Mise (for tool version management)

## Setup

The project is already configured and integrated into the monorepo's Bazel setup.

### Install Dependencies

```bash
cd angular-test
pnpm install
```

### Build

```bash
# From the repository root
bazel build //angular-test:dist
```

The built files will be in `bazel-bin/angular-test/dist/`.

### Development

1. Build the project:
   ```bash
   bazel build //angular-test:dist
   ```

2. Serve the built files:
   ```bash
   cd bazel-bin/angular-test/dist
   python3 -m http.server 8080
   ```

3. Open http://localhost:8080 in your browser

Or use the provided script:
```bash
./angular-test/dev-server.sh
```

## Build Targets

- `//angular-test:bundle` - Builds the JavaScript bundle using esbuild
- `//angular-test:styles` - Processes Tailwind CSS from source files
- `//angular-test:dist` - Combines all assets into a distributable directory

## How It Works

### Tailwind CSS Integration

Tailwind CSS is now fully integrated into the Bazel build process:

1. **Source Scanning**: The `build_tailwind.sh` script scans all `.html` and `.ts` files for Tailwind classes
2. **Dynamic Config**: A temporary config is generated with explicit paths to source files
3. **Processing**: TailwindCSS CLI processes `src/styles.css` and generates the final CSS
4. **Tree Shaking**: Only the Tailwind utilities actually used in your components are included

This means:
- ✅ No manual CSS compilation needed
- ✅ CSS automatically updates when you change HTML/TS files
- ✅ Optimized bundle size (only ~15KB of CSS)
- ✅ Integrated with Bazel's incremental builds

## Configuration Files

### MODULE.bazel

The root MODULE.bazel includes:
- `aspect_rules_js` for JavaScript/Node.js support
- `aspect_rules_esbuild` for bundling
- `aspect_rules_swc` for TypeScript compilation
- NPM dependency translation via pnpm-lock.yaml

### BUILD.bazel

Key build rules:
- `npm_link_all_packages()` - Links NPM dependencies
- `esbuild()` - Bundles TypeScript/JavaScript with Angular
- `copy_to_directory()` - Assembles the final distribution

## Tech Stack

- **Angular**: 19.2.15
- **TypeScript**: 5.6.3
- **Tailwind CSS**: 3.4.18
- **esbuild**: Via aspect_rules_esbuild 0.21.0
- **RxJS**: 7.8.2
- **Zone.js**: 0.15.1

## Notes

- The project uses Angular's standalone components (no NgModules)
- esbuild compiles TypeScript natively without a separate transpilation step
- **Important**: Templates must be inlined using `template:` instead of `templateUrl:` because esbuild doesn't process external HTML templates with Angular's compiler
- **Critical**: `zone.js` must be imported first in main.ts, followed by `@angular/compiler` to enable JIT compilation
- Tailwind CSS processing is fully integrated into the Bazel build pipeline
- The build targets ES2020 to ensure compatibility with esbuild's decorator support

## Future Improvements

- [ ] Add hot module replacement (HMR) for development
- [ ] Add unit testing with Jasmine/Karma or Jest
- [ ] Add e2e testing with Playwright
- [ ] Configure production optimizations (minification, tree-shaking)
- [ ] Add CI/CD pipeline configuration
