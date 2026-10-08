---
title: "Making a platform buildable and deployable again"
description: "Removing generation and release complexity from the teams consuming a model-driven architecture platform."
publishDate: 2026-10-01
tags: ["build-systems", "code-generation", "ci-cd", "java"]
type: work
context: Bloomberg
draft: false
---

<!-- Personal essays intentionally use first-person narration. -->
<!-- vale Google.FirstPerson = NO -->
<!-- vale Google.We = NO -->

The model-driven architecture platform already existed when I started working on it. I expected a mature JVM environment. Instead, I found an aging stack, dependencies that could not be upgraded, fragile builds, and complicated environment-specific release branches.

Teams struggled to build, test, patch, and ship their services. That was a problem for a platform whose plans depended on producing many such services. I took the initiative beyond my original assignment to modernize the stack: removing Apache Camel, upgrading Spring Boot from 2 to 3, and simplifying the interfaces developers worked with. I also shared templates, documentation, and reusable libraries across the organization.

## Generate once, consume a JAR

One immediate threat was the removal of Python 2, which powered the model-to-Java code generation. I first migrated generation to Python 3. Then I centralized the Python generation code in one repository, built, and packaged the generated Java as JARs, and distributed those to consumers.

Downstream Java teams no longer needed Python or the generation toolchain to build their services. They could consume the generated Java as a dependency. The generation machinery still existed, but maintaining and running it was no longer part of every consuming team's build.

## Make shipping part of the main branch

I replaced environment-specific release branches with PR checks and a main-branch release flow. Builds and integration tests run in Docker, with Gradle remote build caching. Merging to main produces a deployable artifact and triggers Bloomberg's automated rollout tooling.

<!-- The paired alternatives or compound predicates are not three-item lists. -->
<!-- vale Google.OxfordComma = NO -->

The replacement is in production and, in my experience, stable and performant, with continuing adoption. The useful change was removing generation and release complexity from consuming teams: give them artifacts they can build against and a path from a checked change to a deployable service.
<!-- vale Google.OxfordComma = YES -->

<!-- vale Google.FirstPerson = YES -->
<!-- vale Google.We = YES -->
