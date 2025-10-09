# Important Notes for Angular + esbuild Setup

## Template Handling

When using esbuild to bundle Angular applications without the Angular CLI, there are important considerations:

### Issue: External Templates (templateUrl)
esbuild does **not** process Angular's `templateUrl` references through the Angular compiler. This means:
- External HTML templates won't be compiled
- Angular can't generate the necessary metadata
- Results in JIT compilation errors at runtime

### Solution: Inline Templates
Use inline `template:` strings instead of `templateUrl:`:

```typescript
// ❌ Don't use with esbuild
@Component({
  selector: 'app-root',
  templateUrl: './app.component.html',
})

// ✅ Use this instead
@Component({
  selector: 'app-root',
  template: `
    <div>Your HTML here</div>
  `,
})
```

### Why This Works
1. Inline templates are included in the TypeScript source
2. The Angular compiler (imported via `@angular/compiler`) can process them at runtime
3. JIT compilation happens in the browser with full template syntax support

### Import Angular Compiler
Add this to your `main.ts`:
```typescript
import '@angular/compiler';
```

This ensures the JIT compiler is available for runtime template compilation.

## Alternative: Use Angular CLI

For production applications with many components, consider:
- Using `@angular-devkit/build-angular` with Bazel
- Pre-compiling with `ngc` (Angular Compiler) before bundling
- Using rules_angular (if available for Bazel)

These approaches provide proper AOT (Ahead of Time) compilation where templates are compiled at build time, resulting in:
- Smaller bundle sizes
- Faster runtime performance
- Better type checking
- No need for JIT compiler in production

## Current Setup Trade-offs

**Advantages:**
- ✅ Simple setup with esbuild
- ✅ Fast builds
- ✅ No complex Angular CLI integration needed
- ✅ Works well for small applications

**Limitations:**
- ⚠️ Templates must be inlined (can be verbose)
- ⚠️ JIT compiler included in bundle (~extra 500KB)
- ⚠️ Runtime template compilation (slightly slower)
- ⚠️ No template type checking at build time

For this Hello World demo, the inline template approach is perfectly suitable and keeps the setup simple!
