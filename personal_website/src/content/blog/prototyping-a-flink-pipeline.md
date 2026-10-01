---
title: "Prototyping a team's first JVM pipeline"
description: "Taking a Flink proof of concept from business logic to dummy traffic on Kubernetes, and helping a team learn the JVM."
publishDate: 2026-10-01
tags: ["flink", "kotlin", "kubernetes", "developer-experience"]
type: work
featured: true
context: Bloomberg
draft: false
---

In 2022, an existing end-of-day C++ pipeline prompted the team to explore a more real-time aggregation system. I built a proof of concept using Flink. The team was unfamiliar with Java, this was its first JVM service, and the deployment was also moving from traditional servers to Kubernetes.

Getting a functional prototype running meant more than porting the business logic. I wrote the application code, first in Java and then in Kotlin, set up the builds and container images, connected CI/CD, and deployed it to Kubernetes. By the end, dummy traffic was flowing through Flink from end to end.

Teaching the team the JVM, Java, Kotlin, and Kubernetes took substantial effort alongside building the prototype. There was a lot to learn at once, and a working application was only one part of making the new environment approachable.

The team subsequently improved the prototype and deployed it to production about a year later. That later rollout was the team's work; my contribution was the functional end-to-end prototype and helping the team get started with the ecosystem.

This was also my first contact with Bloomberg's JVM Guild. The help I received from the Guild, alongside the friction I encountered in the ecosystem, made me want to get involved in the community. It led me toward improving the experience for the next people trying to find their way through it.
