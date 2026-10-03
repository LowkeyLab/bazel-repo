# Browser fixtures

The Bazel `fixtures_build` target injects test routes into a separate Astro build. The production build, sitemap, and image do not include these routes. Each browser test starts its own static server on an ephemeral loopback port and a fresh Chromium context. Local Nix shells supply `CHROMIUM_EXECUTABLE`; remote execution uses the pinned Microsoft Playwright 1.63.0 Noble image and its bundled browser.

Browser tests use the same embedded variable Caveat font as production, with all external network requests blocked. There is no separate test font or Google Fonts substitution. For delayed, held, and blocked font cases, the harness extracts the production font bytes from the built CSS and serves them through a controlled local route. Normal tests exercise the unmodified embedded font.

Font provenance is recorded in `src/assets/fonts/README.md`; the SIL Open Font License is published at `/caveat-OFL.txt`.

Run the four browser targets with `nix develop --command aspect test //personal_website/tests/browser:tests`. The homepage target serves the normal production build while the three component targets serve the separate fixture build. Every target starts its own loopback server, so an already running preview is unnecessary. For a local Chromium smoke run use `nix develop --command aspect test //personal_website/tests/browser:tests --strategy=TestRunner=local --test_env=CHROMIUM_EXECUTABLE`; the Nix shell supplies the executable path. The normal command uses the declared remote browser image.
