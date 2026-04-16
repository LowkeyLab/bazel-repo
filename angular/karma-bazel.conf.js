'use strict';
// Karma configuration for Bazel coverage integration.
//
// When run via `bazel coverage`, this config:
//   1. Writes lcov to a temp file via karma-coverage
//   2. On process exit, rewrites SF: paths to workspace-relative form
//      (e.g. src/app/app.ts → angular/projects/predix/src/app/app.ts)
//      so Bazel's coverage aggregator finds them.
//
// When run outside bazel coverage (e.g. `bazel test` or `ng test`), writes
// an HTML coverage report to a local coverage/ directory.

const path = require('path');
const fs = require('fs');

module.exports = function (config) {
  const coverageOutputFile = process.env['COVERAGE_OUTPUT_FILE'];

  const baseConfig = {
    basePath: '',
    frameworks: ['jasmine'],
    plugins: [
      require('karma-jasmine'),
      require('karma-chrome-launcher'),
      require('karma-jasmine-html-reporter'),
      require('karma-coverage'),
    ],
    jasmineHtmlReporter: {
      suppressAll: true,
    },
    reporters: ['progress', 'kjhtml'],
    browsers: ['Chrome'],
    customLaunchers: {
      // Chrome configured to run in a Bazel sandbox.
      ChromeHeadlessNoSandbox: {
        base: 'ChromeHeadless',
        flags: ['--no-sandbox', '--headless', '--disable-gpu', '--disable-dev-shm-usage'],
      },
    },
    restartOnFileChange: true,
  };

  if (!coverageOutputFile) {
    config.set({
      ...baseConfig,
      coverageReporter: {
        dir: path.join(__dirname, 'coverage'),
        subdir: '.',
        reporters: [{ type: 'html' }, { type: 'text-summary' }],
      },
    });
    return;
  }

  // In bazel coverage mode:
  // - __dirname is <execroot>/angular/ (where this file lives)
  // - process.cwd() is <execroot>/angular/projects/<project> (chdir from js_test)
  // So path.relative(__dirname, '..') gives the Bazel workspace exec root, and
  // path.relative(execRoot, cwd) gives the workspace-relative project path
  // (e.g. "angular/projects/predix").
  const execRoot = path.resolve(__dirname, '..');
  const projectPrefix = path.relative(execRoot, process.cwd()); // e.g. "angular/projects/predix"
  const tmpFile = coverageOutputFile + '.tmp.lcov';

  config.set({
    ...baseConfig,
    coverageReporter: {
      type: 'lcovonly',
      dir: path.dirname(tmpFile),
      file: path.basename(tmpFile),
      subdir: '.',
    },
  });

  // After karma finishes and coverage is written, rewrite SF: paths to
  // workspace-relative form so Bazel's combined coverage report is correct.
  process.on('exit', () => {
    try {
      if (!fs.existsSync(tmpFile)) return;
      const raw = fs.readFileSync(tmpFile, 'utf8');
      // Prepend projectPrefix to SF: lines that don't already start with /
      // (i.e. relative paths like "src/app/app.ts" → "angular/projects/predix/src/app/app.ts")
      const fixed = raw.replace(/^SF:(?!\/)/gm, `SF:${projectPrefix}/`);
      fs.writeFileSync(coverageOutputFile, fixed);
    } catch (_) {
      // Fallback: copy raw file to avoid missing output
      try {
        fs.copyFileSync(tmpFile, coverageOutputFile);
      } catch (_) {}
    }
  });
};
