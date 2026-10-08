---
title: "Free-DSL"
description: "An annotation processor for creating Builders in Kotlin"
publishDate: 2024-08-31
tags: ["kotlin", "code-generation", "dsl"]
type: project
draft: false
---

<!-- Personal essays intentionally use first-person narration. -->
<!-- vale Google.FirstPerson = NO -->
<!-- vale Google.We = NO -->

I built Free-DSL to generate idiomatic Kotlin DSL builders for classes. It uses Kotlin Symbol Processing (KSP) to turn annotations into extension functions and builder classes, giving callers a type-safe syntax for constructing instances.

The generator supports data classes and regular classes with primary constructors, including nested DSL structures, nullable properties, and default values. It works with Kotlin Multiplatform projects.

## Learning code generation with Kotlin

Kotlin is, simply put, the better Java. Many common Java idioms are simply a
language feature in Kotlin. This greatly reduces the boilerplate needed to write
Java-like code. As a fan of Object Oriented Programming (mostly because I
haven't learned how to design in any other paradigm), I love Java, and love
Kotlin even more.

This project is also my first foray into writing a compiler plugin using
the [Kotlin Symbol Processing](https://kotlinlang.org/docs/ksp-overview.html)
API.

## Links

- [Source code](https://github.com/LowkeyLab/gradle-monorepo/tree/main/free-dsl)

<!-- vale Google.FirstPerson = YES -->
<!-- vale Google.We = YES -->
