# Local fonts

These files preserve the accepted prototype inputs from commit
`e77bd0e44dee2e48bd5f142812cf8a44538c1cf3`.
The prototype recorded their source as the Google Fonts repository on 2026-10-23.

- `literata.ttf` supplies headings. Its license is `literata-OFL.txt`.
- `source-sans-3.ttf` supplies body text and controls. Its license is `source-sans-3-OFL.txt`.

The shared presentation library declares all four inputs to Bazel.
Topcoat packages the fonts and licenses as local assets.
The package integrity manifest covers their bytes.
The app makes no remote font request.
