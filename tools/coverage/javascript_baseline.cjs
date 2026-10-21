const path = require("node:path");

// The converter analyzes an empty profile directory. It does not profile itself.
process.env.COVERAGE_DIR = process.env.JAVASCRIPT_BASELINE_DIRECTORY;
require(
  path.resolve(
    process.env.JS_BINARY__RUNFILES,
    process.env.JS_BINARY__WORKSPACE,
    process.env.NATIVE_JAVASCRIPT_COLLECTOR,
  ),
);
