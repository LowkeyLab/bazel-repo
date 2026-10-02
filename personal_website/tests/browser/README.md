# Browser fixtures

The Bazel `fixtures_build` target injects test routes into a separate Astro build. The production build, sitemap, and image do not include these routes. Each browser test starts its own static server on an ephemeral loopback port and a fresh Chromium context. Local Nix shells supply `CHROMIUM_EXECUTABLE`; remote execution uses the pinned Microsoft Playwright 1.63.0 Noble image and its bundled browser.

The Caveat test font comes from [Google Fonts](https://fonts.google.com/specimen/Caveat), downloaded from `fonts.gstatic.com` with a SHA-256 pin in `MODULE.bazel`. Caveat is by Pablo Impallari and is distributed under the [SIL Open Font License 1.1](https://github.com/google/fonts/blob/main/ofl/caveat/OFL.txt). Browser tests serve this declared file locally through request interception; production retains the CDN font.
