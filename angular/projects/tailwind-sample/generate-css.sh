#!/bin/bash
# Script to regenerate TailwindCSS for the tailwind-sample project
# This script should be run with: bazel run //angular/projects/tailwind-sample:generate-css

set -euo pipefail

# Navigate to the workspace root
cd "$BUILD_WORKSPACE_DIRECTORY"

echo "Regenerating TailwindCSS for tailwind-sample project..."

# Ensure pnpm packages are installed
echo "Installing dependencies..."
bazel run -- @pnpm//:pnpm --dir $PWD/angular install

# Run TailwindCSS CLI
echo "Running TailwindCSS CLI..."
cd angular
npx @tailwindcss/cli \
    --input projects/tailwind-sample/src/styles.source.css \
    --output projects/tailwind-sample/src/styles.css

echo "✓ TailwindCSS generated successfully!"
echo "Generated file: angular/projects/tailwind-sample/src/styles.css"
echo ""
echo "The CSS has been regenerated based on the utility classes used in your templates."
echo "Don't forget to commit the updated styles.css file."
