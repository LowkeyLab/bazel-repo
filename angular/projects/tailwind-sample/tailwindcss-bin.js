#!/usr/bin/env node
// Wrapper that locates and executes the bin from @tailwindcss/cli package.json
const { execFileSync } = require('child_process');
const path = require('path');

// Resolve package.json and get bin entry
const pkgPath = require.resolve('@tailwindcss/cli/package.json');
const pkg = require(pkgPath);
const binPath = path.join(path.dirname(pkgPath), pkg.bin.tailwindcss || pkg.bin);

// Get workspace directory (set by `bazel run`)
const workspace = process.env.BUILD_WORKSPACE_DIRECTORY;
if (!workspace) {
  console.error('Error: Run with `bazel run`');
  process.exit(1);
}

// Generate CSS for tailwind-sample project
const input = path.join(workspace, 'angular/projects/tailwind-sample/src/styles.source.css');
const output = path.join(workspace, 'angular/projects/tailwind-sample/src/styles.css');

console.log('Generating TailwindCSS...');
execFileSync(process.execPath, [binPath, '--input', input, '--output', output], {
  stdio: 'inherit',
  cwd: workspace,
});
console.log('✓ Done! Remember to commit styles.css');
