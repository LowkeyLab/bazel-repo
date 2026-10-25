# Markdown checks

Before you complete prose changes, run this command from the repository root:

```bash
nix develop --command aspect test //tools/lint:vale_test
```

This command checks the complete configured Markdown baseline, including
`book_smartz/web/README.md`. It uses the repository Vale configuration and styles.
It checks all lines, including lines outside the current diff.

A clean baseline returns exit code zero. A blocking Vale diagnostic returns a
nonzero exit code. Advisory suggestions do not cause failure.
Strict ASD-STE100 uses full forms, so `Google.Contractions` is off.
Other configured styles and severities remain active.

The required CI test job runs `aspect test //...`. This selection includes
`//tools/lint:vale_test` and the tests for the diagnostic policy.
The normal Aspect lint command uses a changed-line strategy by default.
Run the command in this document for complete prose validation.
