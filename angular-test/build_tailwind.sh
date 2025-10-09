#!/usr/bin/env bash
set -euo pipefail

# Arguments:
# $1 - input CSS file
# $2 - output CSS file  
# $3 - tailwind config file
# All remaining args - source files

INPUT_CSS="$1"
OUTPUT_CSS="$2"
TAILWIND_CONFIG="$3"
shift 3

# Find the node_modules directory in the runfiles
RUNFILES_DIR="${RUNFILES_DIR:-$0.runfiles}"
NODE_MODULES="${RUNFILES_DIR}/_main/angular-test/node_modules"

# Set up Node environment
export NODE_PATH="${NODE_MODULES}"
export HOME="${HOME:-/tmp}"

# Find tailwindcss binary
TAILWINDCSS_BIN="${NODE_MODULES}/.bin/tailwindcss"

if [ ! -f "${TAILWINDCSS_BIN}" ]; then
    # Try alternative location
    TAILWINDCSS_BIN="${NODE_MODULES}/tailwindcss/lib/cli.js"
fi

# Get the directory of the input config
CONFIG_DIR="$(dirname "${TAILWIND_CONFIG}")"

# Create a temporary config that includes all source files explicitly
TEMP_CONFIG="${OUTPUT_CSS}.config.js"
cat > "${TEMP_CONFIG}" <<EOF
export default {
  content: [
$(for file in "$@"; do
  # Only include HTML and TS files
  if [[ "$file" == *.html ]] || [[ "$file" == *.ts ]]; then
    echo "    '${file}',"
  fi
done)
  ],
  theme: {
    extend: {},
  },
  plugins: [],
}
EOF

# Run tailwindcss with the temporary config
"${TAILWINDCSS_BIN}" \
    --input "${INPUT_CSS}" \
    --output "${OUTPUT_CSS}" \
    --config "${TEMP_CONFIG}"

# Clean up
rm -f "${TEMP_CONFIG}"
