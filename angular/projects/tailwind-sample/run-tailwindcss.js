#!/usr/bin/env node
/**
 * Wrapper to invoke TailwindCSS CLI from the @tailwindcss/cli package
 * This executes the CLI bin entry defined in the package.json
 * 
 * When run via `bazel run`, it regenerates the TailwindCSS for the project.
 */

const { execFileSync } = require('child_process');
const path = require('path');

try {
  // Load the @tailwindcss/cli package.json to get the bin entry
  const cliPkgPath = require.resolve('@tailwindcss/cli/package.json');
  const cliPkg = require(cliPkgPath);
  const cliPkgDir = path.dirname(cliPkgPath);
  
  // Get the bin entry - it's an object with key 'tailwindcss'
  const binPath = path.join(cliPkgDir, cliPkg.bin.tailwindcss || cliPkg.bin);
  
  // When run via `bazel run`, BUILD_WORKSPACE_DIRECTORY is set
  const workspaceDir = process.env.BUILD_WORKSPACE_DIRECTORY;
  
  if (!workspaceDir) {
    console.error('Error: This tool should be run with `bazel run`');
    process.exit(1);
  }
  
  // Construct paths relative to workspace
  const inputPath = path.join(workspaceDir, 'angular/projects/tailwind-sample/src/styles.source.css');
  const outputPath = path.join(workspaceDir, 'angular/projects/tailwind-sample/src/styles.css');
  
  console.log('Regenerating TailwindCSS for tailwind-sample project...');
  console.log(`Input:  ${inputPath}`);
  console.log(`Output: ${outputPath}`);
  
  // Execute the CLI
  execFileSync(process.execPath, [
    binPath,
    '--input', inputPath,
    '--output', outputPath
  ], {
    stdio: 'inherit',
    cwd: workspaceDir
  });
  
  console.log('✓ TailwindCSS generated successfully!');
  console.log('Don\'t forget to commit the updated styles.css file.');
} catch (error) {
  console.error('Failed to execute TailwindCSS CLI:', error.message);
  process.exit(1);
}
