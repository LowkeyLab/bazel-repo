#!/usr/bin/env node
// Build-time wrapper for TailwindCSS that generates CSS from source files.
// Used by js_run_binary in Bazel to generate the styles.css file.
const { execFileSync } = require('child_process');
const fs = require('fs');
const path = require('path');

// Resolve package.json and get bin entry
const pkgPath = require.resolve('@tailwindcss/cli/package.json');
const pkg = require(pkgPath);
const binPath = path.join(path.dirname(pkgPath), pkg.bin.tailwindcss || pkg.bin);

// Parse command line arguments
const args = process.argv.slice(2);
if (args.length < 2) {
  console.error('Usage: tailwindcss-generate.js <input> <output>');
  process.exit(1);
}

const input = args[0];
let output = args[1];

// When running from BAZEL_BINDIR (the default for js_binary), the output path
// is relative to execroot but we're in bindir. We need to adjust the output path
// to avoid double-nesting (bindir/bindir/...).
const bazelBinDir = process.env.BAZEL_BINDIR;
if (bazelBinDir && output.startsWith(bazelBinDir + '/')) {
  // Remove the bindir prefix since we're already in bindir
  output = output.slice(bazelBinDir.length + 1);
}

// Create output directory if it doesn't exist
const outputDir = path.dirname(output);
if (outputDir && !fs.existsSync(outputDir)) {
  fs.mkdirSync(outputDir, { recursive: true });
}

console.log(`Generating TailwindCSS: ${input} -> ${output}`);
execFileSync(process.execPath, [binPath, '--input', input, '--output', output], {
  stdio: 'inherit',
});
console.log('✓ TailwindCSS generation complete');
