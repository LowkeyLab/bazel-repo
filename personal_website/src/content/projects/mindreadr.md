---
title: "Mindreadr"
description: "A cooperative word-guessing game"
cover: "../../assets/mindreadr.png"
tags: ["Kotlin", "Ktor", "Angular", "TypeScript", "WebSocket", "Bazel"]
featured: true
links:
  github: "https://github.com/tacascer/bazel-repo/tree/main/mindreadr"
  demo: "https://mindreadrfrontend-production.up.railway.app/"
---

## Overview

Mindreadr is a cooperative multiplayer game where two players try to arrive at the same word as each other.

This is a rewrite of [guess-the-word](./guess-the-word), with a different tech stack and more self-hosted infrastructure. Hopefully this time I won't get my infrastructure decommisioned by someone else (looking at you, Supabase). Oh, and some new game mechanics were added, too.

## Features

- Create rooms and play with friends.
- Real-time communication between players.
- Beautiful UI on desktop and mobile.

## Tech Stack

### Kotlin & Ktor

The backend is built using **Kotlin** and the [**Ktor**](https://ktor.io/) framework.

Ktor's native support for Kotlin [coroutines](https://kotlinlang.org/docs/coroutines-overview.html) allow me to utilize a coroutine and channel based model of communication.
This model of communication is extremely well suited for a multiplayer game, where multiple clients need to send messages to each other as the game state changes.

#### Why not Spring Boot?

The juggernaut in the JVM space is obviously [Spring Boot](https://spring.io/projects/spring-boot). So then, why did I not consider it?

Spring Boot is ... heavy. Its emphasis on reflection allows developers to write applications with very few lines of code, relying mostly on annotations. However, those annotations do a lot, much more than the average Spring Boot developer would know. Unless, of course, the application crashes because some bean wasn't initialized in the right order.

Having debugged many a Spring application (mine and others), I yearn for a more direct, less "magical" way to write applications. Ktor fits that style of programming, and supports coroutines. What's not to love?

#### The resource problem

Since Ktor strips out a lot of reflection magic, I was expecting it to either:

- Be faster
- Be more efficient (memory-wise)

Unfortunately, it seems that Ktor succumbs to the JVM curse of using too much memory to just ... be idle.

My currently deployed service takes up at least 100MB of RAM on start-up without any traffic.

It also has an intial health check latency of around 70ms. Mind you, this is an endpoint that just returns HTTP 200 OK. Nothing else. And it takes 70ms to do so on start-up.

Of course, afterwards the JIT kicks in, and the responses come back in around 1ms. Still, that is still unacceptably slow for consuming at least 100MB of RAM.

### Angular & TypeScript

Angular seems to be the only front-end framework that fully embraces object-oriented (OOP) design . Hell, it even has support for [dependency injection](https://angular.dev/essentials/dependency-injection), a concept mostly seen only in OOP software.

Angular also comes with native Typescript support, making its programming model class-based. This is very different from the functional-ish design that React has been spreading, and so might appear unnatural to the average JavaScript programmer. I, on the other hand, love it.

Angular's testability is amazing. It usually is non-trivial to try and inject fake results from calling the backend into a frontend test, but Angular's support for dependency injection makes this a non-issue.

Angular's tooling (with its CLI) is also top-notch. Its focus on automated refactoring and code generation align with my vision of what a good developer experience should look like. Unfortunately, it sometimes doesn't work with [Bazel](#bazel).

### Bazel

[Bazel](https://bazel.build/) is Google's build tool. It forces your builds to be hermetic, and in return you get very aggressive caching of builds.

Bazel is notorious for being hard to use. I've yet to see someone picking Bazel as their build system without having worked with it previously (e.g. at Google, or Uber, etc.)

Since it's a build system designed for monorepos, the open source ecosystem is very small. If you are looking to start your Bazel journey, I will recommend some resources that should be enough to build whatever code you're writing.

Once I've ironed out all the quirks (and there were many), I can safely say that Bazel's caching is extremely good. I've since migrated all of my personal projects (yes, everything you see on this site) onto the same Bazel repo.

I've also dabbled in [remote build execution](https://bazel.build/remote/rbe) (RBE). For a while, I was able to run my tests in parallel, taking only 10 seconds (!) to run all the tests of all my personal projects.

However, some C/C++ toolchain incompatibility cropped up, and I had to give up on RBE to continue running my tests 😿. Maybe one day Bazel's C/C++ [toolchain support](https://bazel.build/extending/toolchains) will provide a seamless experience for setting up RBE. For now, I'll just have to settle for the remote build cache.

#### Useful Resources

Some resources that aren't just the Bazel [documentation](https://bazel.build/docs) that I found extremely helpful:

- [The Bazel 101 series](https://www.youtube.com/watch?v=LMsTPYO0jTU&list=PLLU28e_DRwdswrrZaNqnFFm9OawpxN4CB) will give you a better understanding of Bazel than the built-in tutorials.
- [Rules Lint](https://github.com/aspect-build/rules_lint) allows you to set up unified linting and formatting for your (mono)repo.
- [The Aspect CLI](https://github.com/aspect-build/aspect-cli), which gives you the `lint` command to run the linters set up in rules_lint.
- [The Bazel Starters](https://github.com/bazel-starters) to see what a well-structured Bazel build for your language looks like
- [The Bazel frontend examples](https://github.com/bazelbuild/examples/tree/main/frontend), because building frontend code looks different from all the other kinds of code in Bazel. Strangely enough, you won't find an example for Angular here.
- [Rules Angular](https://github.com/devversion/rules_angular) if you are looking to also build your Angular app with Bazel. Surprisingly, Bazel doesn't have very good support for Angular (despite both being from Google).
- [Sample Angular Project With Tailwind](https://github.com/LowkeyLab/bazel-repo/tree/main/angular/projects/tailwind-sample) if you want to see how to use TailwindCSS with Angular and Bazel. Surprisingly, I couldn't find this specific example anywhere on the internet, and had to go through [many iterations](https://github.com/LowkeyLab/bazel-repo/commits/main/angular/projects/tailwind-sample) to get it working.
- [Gazelle](https://github.com/bazel-contrib/bazel-gazelle) to generate BUILD files automatically for supported languages. In my experience, it has only worked well for Go.
- [Buildbuddy](https://www.buildbuddy.io/) for a free remote build cache.
