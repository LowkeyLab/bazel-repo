# Hearthstone Simulator

A deterministic, headless Hearthstone-style simulator scaffold using Bevy's ECS and app scheduler. It intentionally excludes rendering so the core can run in tests, servers, AI search, or a future UI.

## Included

- Bevy `App`, plugin, resources, components, and update system
- Two-player turn and mana progression
- Minion cards, seven-slot boards, summoning sickness, and hero attacks
- Typed actions, errors, and deterministic snapshots
- Unit tests and a small scripted CLI example

This is an architectural starting point, not a complete implementation of Hearthstone rules or official card data. Decks, drawing, spells, minion combat, keywords, triggers, random effects, and card definitions are natural next modules.

## Commands

```bash
aspect test //hearthstone_simulator/core:core_test
bazel run //hearthstone_simulator/app
bazel run //tools/coverage -- //hearthstone_simulator/...
```

The coverage command writes the combined LCOV data to `coverage-report.lcov` and, when `genhtml` is available, generates an HTML report at `coverage-report/index.html`. The repository coverage workflow also uploads the simulator's coverage to Codecov.

All repository operations should continue to use Bazel/Aspect rather than invoking Cargo directly.
