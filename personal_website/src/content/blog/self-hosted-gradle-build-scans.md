---
title: "Self-Hosted Gradle Build Scans with LowkeyLab"
description: "A technical tour of gradle-build-scan-server: a self-hosted way to collect, inspect, and learn from Gradle build scans."
publishDate: 2026-05-27
tags: ["gradle", "build-tools", "rust", "angular", "observability"]
draft: false
---

# Self-Hosted Gradle Build Scans

Gradle offers
[remote build scans](https://docs.gradle.org/current/userguide/inspect.html)
that show very, very useful information about your build. Information like cache
hits, misses, execution times, etc. is available for inspection. These details
help you debug builds, cache misses, and more.

A build scan is a mandatory tool in a build engineer’s toolkit.

The problem: To create a Gradle build scan, you must either use the
[public scan endpoint](https://gradle.com/scans/gradle/), or you must get a
[Develocity](https://gradle.com/pricing/) instance.

I needed to inspect build details without a Develocity instance and without
needing to stream my build over the internet to the public endpoint.
Privacy-oriented developers would also have the same need.

So, I decided to write a
[self-hosted Gradle Build Scan server](https://github.com/LowkeyLab/gradle-build-scan-server)
that tries its best to capture important information about your Gradle build.

## What It Is

The server accepts scans published by the Develocity Gradle plugin and stores
them locally. You can run it with Docker, point a Gradle build at it, and open
the web UI at `/web/scans`.

```bash
docker run --rm -p 8080:8080 -v build-scan-data:/data ghcr.io/lowkeylab/build-scan-server:latest
```

A local demo setup can be as small as adding the Develocity plugin in
`settings.gradle.kts`, pointing it at the server, and disabling background
upload so the scan is sent before the build exits.

```kotlin
plugins {
  id("com.gradle.develocity") version "4.3.2"
}

develocity {
  server = "http://localhost:8080"
  buildScan {
    uploadInBackground = false
  }
}
```

## What You Can Inspect

The scan list gives you a quick view of uploaded builds, and each detail page
lives under `/web/scans/<scan-id>`. From there, the UI focuses on the data I
usually want when a build gets weird:

- build outcome, timestamps, requested tasks, Gradle version, plugin version,
  host, OS, and JVM details
- task paths, outcomes, class names, timing, duration, and dependency data
- cacheability, cache keys, up-to-date messages, and disabled caching
  explanations
- cache operations with their kind, outcome, duration, and cache key when Gradle
  provides one
- test counts, summary badges, and test cases with class, method, executor,
  outcome, duration, failure message, and stack trace when available

## Where to Find It

The code is on [GitHub](https://github.com/LowkeyLab/gradle-build-scan-server).

## Afterthoughts

It's unfortunate that such crucial tooling for build system diagnostics is
hidden behind a paywall, terms of service, or a combination of both. As Gradle
is one of the most popular build systems for the vast JVM ecosystem, it is
worrying that such crucial information for build system optimizations is hidden
behind closed-source, paywalled code.

In fact, not even the wire protocol of the build scan is documented. It turns
out to be Kryo-based, and although the source code for it is not available, it
can be decompiled from the downloaded JARs that come as part of using the build
plugin.
_hint_ _hint_

Gradle's obfuscation of these crucial diagnostics stands in stark contrast to
the open, documented
[Build Event Protocol](https://bazel.build/remote/bep) of Bazel, another big
player in the JVM build system space.

Hopefully, as Bazel gains more traction, Gradle will be pressured to open the
build scan protocol. As of now, reverse engineering it is your best bet. I've
sort of reimplemented Kryo in Rust as part of this process, so it might be worth
studying and may inspire future work integrating Kryo with Rust serde.

Thanks for reading, and try it out!
