---
title: "Free-DSL"
description: "An annotation processor for creating Builders in Kotlin"
cover: "../../assets/free-dsl.png"
tags: ["Kotlin"]
featured: true
links:
  github: "https://github.com/LowkeyLab/gradle-monorepo/tree/main/free-dsl"
---

## Overview

Free-DSL is a Kotlin Multiplatform library that generates idiomatic Kotlin DSL
builders for data classes and regular classes with primary constructors. It uses
Kotlin Symbol Processing (KSP) to generate extension functions and builder
classes that enable a clean, type-safe DSL syntax for constructing instances of
your classes.

## Features

- Simple annotation-based API
- Generates idiomatic Kotlin DSL builders
- Supports both data classes and regular classes with primary constructors
- Supports nested DSL structures
- Handles nullable properties and default values
- Works with Kotlin Multiplatform projects

## Tech Stack

### Kotlin

Kotlin is, simply put, the better Java. Many common Java idioms are simply a
language feature in Kotlin. This greatly reduces the boilerplate needed to write
Java-like code. As a fan of Object Oriented Programming (mostly because I
haven't learned how to design in any other paradigm), I love Java, and love
Kotlin even more.

This project is also my first foray into writing a compiler plugin using
the [Kotlin Symbol Processing](https://kotlinlang.org/docs/ksp-overview.html)
API.
