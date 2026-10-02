---
title: "Building JVM RPC tooling around Bloomberg's needs"
description: "Finding the boundary between useful open-source frameworks and the RPC behavior Bloomberg needs."
publishDate: 2026-10-01
tags: ["kotlin", "rpc", "serialization", "code-generation"]
type: work
featured: true
context: Bloomberg
draft: false
---

The JVM community at Bloomberg had tried adapting entire open-source solutions to our internal technologies. That brought incompatibilities and performance issues. Some of the foundational RPC libraries, including codecs, were unmaintained, with bugs that had gone unfixed for years.

I developed a Kotlin-based JVM code generator and codec around Bloomberg's requirements. Better type safety, performance, and cross-language compatibility were the goals. There were concrete problems to work on: the old XJC-based tooling could not represent `xs:choice` as sealed type hierarchies, and the old BER codec had subtle behavioral differences from the C++ and Python implementations.

## Choosing what to reuse

Bloomberg's JSON, XML, and BER behavior did not fit the approach of reusing an entire stack. That did not mean every useful piece of open-source software had to go.

The new generator supports Jackson and kotlinx.serialization. KotlinPoet handles Kotlin source generation. Jackson gives me a framework for implementing Bloomberg data formats, while kotlinx.serialization provides a serialization framework with custom encoders and decoders. Those are useful foundations on which to implement the behavior our environment needs.

The tooling is in limited beta and moving toward production.

## A better boundary

This project expanded my understanding of code generation, schemaful and schemaless formats, serialization, wire formats, and performance optimization at both the language and hardware level.

What I took away is a more deliberate approach to reuse. Keep the frameworks that help, implement the behavior specific to the environment, and be willing to inspect or adapt open-source code with appropriate attribution and licensing. Blind reuse and blind rewrites both miss that boundary.
