# TailwindCSS Integration with Bazel

## Overview

TailwindCSS is now fully integrated into the Bazel build process for the angular-test project. No more manual CSS compilation is needed - Tailwind processing happens automatically as part of the build.

## How It Works

### Build Flow

```
Source Files (*.html, *.ts)
         ↓
  [Bazel Build]
         ↓
  tailwind_srcs (filegroup) → Collects all HTML/TS files
         ↓
  build_tailwind.sh → Scans files, generates temp config
         ↓
  TailwindCSS CLI → Processes src/styles.css
         ↓
  styles.out.css → Optimized CSS with only used utilities
         ↓
  copy_to_directory → Final dist/styles.css
```

### Key Components

#### 1. `build_tailwind.sh`
A wrapper script that:
- Receives source files as arguments
- Generates a temporary Tailwind config with explicit file paths
- Runs TailwindCSS CLI in the Bazel sandbox
- Ensures all source files are scanned for utility classes

#### 2. `BUILD.bazel` Targets

**`tailwind_srcs` (filegroup)**
```starlark
filegroup(
    name = "tailwind_srcs",
    srcs = glob([
        "src/**/*.html",
        "src/**/*.ts",
    ]),
)
```
Collects all source files that need to be scanned for Tailwind classes.

**`build_tailwind_bin` (sh_binary)**
```starlark
sh_binary(
    name = "build_tailwind_bin",
    srcs = ["build_tailwind.sh"],
    data = [
        ":node_modules/tailwindcss",
        ":node_modules/autoprefixer",
        ":node_modules/postcss",
    ],
)
```
Wraps the build script with necessary npm package dependencies.

**`styles` (genrule)**
```starlark
genrule(
    name = "styles",
    srcs = [
        "src/styles.css",
        "tailwind.config.js",
        "postcss.config.js",
        ":tailwind_srcs",
    ],
    outs = ["styles.out.css"],
    cmd = """
        $(location :build_tailwind_bin) \
            $(location src/styles.css) \
            $@ \
            $(location tailwind.config.js) \
            $(locations :tailwind_srcs)
    """,
    tools = [":build_tailwind_bin"],
)
```
Executes the Tailwind build process within Bazel's sandbox.

## Benefits

### ✅ Automatic Processing
- CSS is regenerated whenever source files change
- No manual `pnpm exec tailwindcss` commands needed
- Integrated with Bazel's incremental build system

### ✅ Tree Shaking
- Only Tailwind utilities actually used in your code are included
- Result: ~15KB CSS file (vs ~3MB full Tailwind)
- Automatically updates as you add/remove classes

### ✅ Build Caching
- Bazel caches the CSS generation
- Rebuilds only when source files change
- Fast incremental builds

### ✅ Sandbox Isolation
- Build runs in isolated environment
- Reproducible across different machines
- No interference with local environment

## Usage

### Normal Development
Just build as usual:
```bash
bazel build //angular-test:dist
```

The CSS will be automatically processed from `src/styles.css` and included in the output.

### Adding New Tailwind Classes
1. Add Tailwind utility classes to your HTML/TypeScript files
2. Run `bazel build //angular-test:dist`
3. The new classes will automatically be included in the generated CSS

No separate compilation step needed!

### Checking Generated CSS
```bash
# View the generated CSS
cat bazel-bin/angular-test/dist/styles.css

# Check CSS size
wc -l bazel-bin/angular-test/dist/styles.css

# Search for specific classes
grep "your-class" bazel-bin/angular-test/dist/styles.css
```

## Configuration

### tailwind.config.js
The project still has a `tailwind.config.js` file, but it's used as a template. The `build_tailwind.sh` script generates a temporary config with explicit paths during the build.

### postcss.config.js
Standard PostCSS configuration that tells TailwindCSS to use autoprefixer:
```javascript
export default {
    plugins: {
        tailwindcss: {},
        autoprefixer: {},
    },
}
```

## Troubleshooting

### Classes Not Appearing in CSS
If you add a Tailwind class but it doesn't appear in the generated CSS:

1. **Rebuild from scratch:**
   ```bash
   bazel clean
   bazel build //angular-test:dist
   ```

2. **Check if the file is included:**
   Make sure your HTML/TS file matches the glob pattern in `tailwind_srcs`:
   - `src/**/*.html`
   - `src/**/*.ts`

3. **Verify the class syntax:**
   Ensure the class name is correctly formatted (e.g., `bg-blue-500` not `bg-blue500`)

### Build Errors
If you see errors during the Tailwind build:

1. **Check node_modules:**
   ```bash
   cd angular-test
   pnpm install
   ```

2. **Verify the script is executable:**
   ```bash
   chmod +x angular-test/build_tailwind.sh
   ```

3. **Check the build log:**
   ```bash
   bazel build //angular-test:styles --verbose_failures
   ```

## Performance

### Build Times
- **Initial build:** ~110ms for CSS processing
- **Incremental builds:** Cached (instant) if no source changes
- **Clean build:** ~5-6 seconds for entire project

### Output Size
- **Generated CSS:** ~15KB (~778 lines)
- **Full Tailwind:** ~3MB (all utilities)
- **Reduction:** ~99.5% smaller

## Migration Notes

### What Changed
- ❌ Removed: `src/styles-processed.css` (pre-compiled CSS)
- ✅ Added: `build_tailwind.sh` (build script)
- ✅ Modified: `BUILD.bazel` (integrated Tailwind processing)
- ✅ Updated: Documentation (README.md, IMPLEMENTATION.md)

### What Stayed the Same
- `src/styles.css` - Still the source file with `@tailwind` directives
- `tailwind.config.js` - Still used (as template)
- `postcss.config.js` - Still used
- Component files - No changes needed

### Breaking Changes
None! The application works exactly the same, just with automated CSS processing.

## Future Enhancements

Potential improvements to the integration:

- [ ] Add CSS minification for production builds
- [ ] Implement content hash for CSS filename (cache busting)
- [ ] Add source maps for debugging
- [ ] Create separate configs for dev vs prod (different purge strategies)
- [ ] Add CSS optimization plugins (PurgeCSS, cssnano)
- [ ] Integrate with watch mode for faster development

## References

- [TailwindCSS Documentation](https://tailwindcss.com/docs)
- [Bazel Documentation](https://bazel.build/docs)
- [aspect_rules_js](https://docs.aspect.build/rulesets/aspect_rules_js/)
