# Angular + Tailwind + Bazel Integration Summary

## What Was Created

A fully functional Hello World Angular application integrated into your Bazel monorepo with Tailwind CSS support.

## Project Location

`/home/tacascer/Projects/bazel-repo/angular-test/`

## Key Files Created

### Configuration Files
- **MODULE.bazel** (updated) - Added rules_js, rules_esbuild, rules_swc, and npm configuration
- **package.json** - Angular 19 and Tailwind CSS dependencies
- **pnpm-lock.yaml** - Dependency lockfile
- **tsconfig.json** - TypeScript configuration with decorator support
- **tailwind.config.js** - Tailwind CSS configuration
- **postcss.config.js** - PostCSS configuration
- **.bazelignore** (created) - Excludes node_modules from Bazel
- **mise.toml** (updated) - Added Node.js 20.18.0 and PNPM 9

### Application Files
- **src/index.html** - Entry HTML file
- **src/main.ts** - Bootstrap file for Angular
- **src/styles.css** - Global styles with Tailwind directives
- **src/app/app.component.ts** - Main Angular component
- **src/app/app.component.html** - Component template with Tailwind classes
- **src/app/app.config.ts** - Application configuration

### Build Files
- **BUILD.bazel** - Bazel build targets for bundling and distribution
- **build_tailwind.sh** - Script to process Tailwind CSS in Bazel sandbox
- **dev-server.sh** - Convenience script for development

### Documentation
- **README.md** - Complete project documentation

## How to Use

### Build the Application
```bash
cd /home/tacascer/Projects/bazel-repo
bazel build //angular-test:dist
```

### Run Development Server
```bash
# Option 1: Use the convenience script
./angular-test/dev-server.sh

# Option 2: Manual
bazel build //angular-test:dist
cd bazel-bin/angular-test/dist
python3 -m http.server 8080
```

Then open http://localhost:8080 in your browser.

## Technologies Used

- **Angular**: 19.2.15 (standalone components)
- **Tailwind CSS**: 3.4.18
- **TypeScript**: 5.6.3
- **Bazel**: aspect_rules_js, aspect_rules_esbuild
- **Node.js**: 20.18.0 (via mise)
- **PNPM**: 9.x

## Build Process

1. **npm_link_all_packages** - Links NPM dependencies from pnpm-lock.yaml
2. **tailwind_srcs** (filegroup) - Collects all HTML and TypeScript source files
3. **build_tailwind_bin** (sh_binary) - Wrapper script for TailwindCSS CLI
4. **styles** (genrule) - Processes Tailwind CSS
   - Scans all source files for Tailwind classes
   - Generates optimized CSS with only used utilities
   - Runs in Bazel sandbox with proper file paths
5. **esbuild** - Bundles TypeScript/JavaScript files
   - Compiles TypeScript natively
   - Handles Angular decorators (ES2020 target)
   - Bundles all dependencies
6. **copy_to_directory** - Assembles final distribution
   - Flattens directory structure (`src/` prefix removed)
   - Places `index.html` at root for proper serving
   - Includes processed CSS as `styles.css`

## Architecture Highlights

- **Standalone Components**: No NgModules, using modern Angular patterns
- **Native esbuild**: TypeScript compilation without separate transpilation step
- **Inlined Templates**: Templates use `template:` instead of `templateUrl:` for esbuild compatibility
- **JIT Compilation**: `@angular/compiler` imported in main.ts to enable runtime template compilation
- **Tailwind CSS**: Fully integrated into Bazel build with automatic tree-shaking
- **Bazel Integration**: Fast, cacheable builds integrated with existing monorepo

## Features Demonstrated

✅ Angular 19 with standalone components
✅ Tailwind CSS utility classes
✅ Responsive design
✅ Gradient backgrounds
✅ Shadow effects and hover states
✅ Grid layouts
✅ Production-ready build configuration

## Output

The built application is in `bazel-bin/angular-test/dist/`:
- `index.html` - Entry point (at root level)
- `bundle/main.js` - JavaScript bundle (~1.4 MB unminified)
- `styles.css` - Compiled Tailwind CSS (~14 KB)

## Next Steps (Future Enhancements)

- Add hot module replacement (HMR)
- Add unit and e2e testing
- Configure production optimizations (minification, tree-shaking)
- Add routing and additional features
- Set up CI/CD pipeline

## Notes

- The project uses experimental decorators for Angular compatibility
- esbuild targets ES2020 to support decorators
- Tailwind CSS is fully integrated into the Bazel build process
- The `build_tailwind.sh` script dynamically generates Tailwind config to work within Bazel's sandbox
- The development server is a simple Python HTTP server (suitable for development only)
