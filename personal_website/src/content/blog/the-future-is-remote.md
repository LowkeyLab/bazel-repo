---
title: "The future is remote"
description: "Your laptop should not be your build infrastructure."
publishDate: 2026-09-28
tags: ["build-tools", "bazel", "remote-execution", "developer-experience"]
draft: false
---

_And no, I'm not talking about remote work_

Your laptop should not be your build infrastructure.

Yet a surprising amount of software development still revolves around that assumption. You type the build command on your machine, so your machine is expected to do the work. Install the toolchains. Download the dependencies. Assemble the environment. Run the build.

When that gets slow, you buy a more powerful laptop.

When onboarding gets painful, you package the environment in a Docker image.

When CI behaves differently, you spend engineering time figuring out which part of the developer’s machine you forgot to reproduce.

We have normalised an expensive way to build software. You should demand better.

The future is remote builds. Investing in that future should be part of your engineering strategy.

## Your cache hit is still doing too much work

Look at your `target/` or `build/` directory. That mutable staging area is where your build assembles its world. Dependencies arrive, intermediate files accumulate, and later steps consume the outputs of earlier ones.

Adding a remote cache helps. But if your build still needs to reconstruct that world locally, you have only solved part of the problem.

Consider a fresh checkout. Your build checks a task’s inputs, finds a cache entry, downloads its outputs, and puts them where subsequent tasks expect them. Those tasks evaluate their inputs and repeat the process. Gradle’s build cache, for example, [restores task outputs this way](https://docs.gradle.org/current/userguide/build_cache.html).

Congratulations: you avoided compilation. You are now waiting for downloads, unpacking, filesystem writes, and more input checks.

A 99.9% cache hit rate can look fantastic on a dashboard while your developers continue staring at a terminal.

An established checkout may already have the files. Your fresh CI workers, disposable development environments, and agent sandboxes often do not. They get to reconstruct the same world again.

You should be asking why those files need to reach your machine in the first place.

## Stop downloading work you do not need

Bazel calls its answer [_Build without the Bytes_](https://blog.bazel.build/2023/10/06/bwob-in-bazel-7.html).

Intermediate outputs can stay in remote storage. Your machine downloads the subset it actually needs.

Suppose a remote compiler produces an object file and a remote linker consumes it. Why should your laptop need a copy between those two operations?

It should not.

The build still needs to analyse dependencies and determine which results are reusable. But it does not need to fill your local filesystem with every intermediate artifact to do useful work.

This should be an ordinary expectation of build infrastructure. Once you see the unnecessary data movement, it becomes difficult to defend making every developer and every fresh environment pay for it.

## Put the compute near the data. We already know this.

We routinely design services around data locality. Then we tolerate build pipelines that drag enormous working sets across a developer’s internet connection.

Apply the same engineering judgement to your builds.

Bazel supports [remote execution of build and test actions](https://bazel.build/remote/rbe). Buck2 supports [execution platforms](https://buck2.build/docs/users/remote_execution/) that describe where and how actions run. These give you the foundation to put workers close to the cache and distribute independent work across a fleet.

If a build action needs a large artifact, let a worker retrieve it over the data centre network. If a hundred independent actions are ready, give them somewhere to run.

You will still have a critical path. You will still need to manage scheduling and storage throughput. But these become infrastructure problems you can address centrally, with benefits shared by everyone using the system.

Buying every developer another expensive machine is a poor substitute for fixing the architecture.

## Hermeticity is an economic necessity

If your build depends on an undocumented compiler installation, an arbitrary environment variable, or something sitting in a developer’s home directory, you have a liability.

Someone will eventually pay to discover that dependency. Usually when a build fails somewhere inconvenient.

[Hermetic builds](https://bazel.build/basics/hermeticity) make dependencies explicit and isolate execution from the surrounding machine. That gives you a foundation for reproducibility and safe reuse.

You should care about this even if you never intend to run a large worker fleet.

Every result you can safely reuse is work another developer, CI job, or agent does not have to repeat. Every environment assumption you eliminate is one fewer thing to investigate when two machines disagree.

You want both shorter feedback loops and less duplicated work. Build infrastructure should be designed to deliver both.

## Stop shipping the build environment to everyone

Instead of shipping the build machines to the code, ship the code to the build machines.

A Docker image or specialised development environment can make setup more consistent. But if everyone still executes the build locally, everyone still needs the storage, memory, and compute to support it.

I have seen companies respond by buying more powerful laptops, bigger development machines, and larger cloud coding environments.

At some point, you have to ask whether you are funding the same workaround over and over.

Put that investment into shared execution. Provision worker pools for the workload. Reuse completed actions. Let developers request builds without first turning their editing environment into a miniature CI installation.

Containers remain useful here. Use them to provide consistent execution environments for shared workers.

And use those same configured environments for developer requests and CI. Stop accepting “it works on my machine” as an inevitable feature of software development. Invest in removing the differences that make it possible.

## Your integration strategy cannot depend on everyone’s laptop

Suppose you change a shared library and want to build and test its dependents.

One uses Java. Another uses Rust. Another requires a native toolchain for a different operating system.

Should validating that change require you to install every team’s prerequisites? Should the scope of your testing depend on how much RAM happens to be in your laptop?

That is an absurd constraint to place on integration.

A shared build graph can describe the affected work. Remote execution infrastructure can send that work to appropriately configured machines. You still need to model dependencies and provision the environments, but you can do that once and make it available across the organisation.

The fleet does not need to be homogeneous. You can have worker pools for different operating systems, architectures, and capabilities.

With suitable toolchain and backend support, you can edit on Linux and request a build on Windows. Your macOS build can run on appropriately provisioned Apple hardware. The machine where you write the code does not have to be the machine qualified to build it.

That separation should be part of how you design your development platform.

## “But we’re not Google”

You do not have to be Google to want fast, cheap, reusable, hermetic, reproducible builds.

These are properties any serious software outfit should pursue. Your company’s size does not make wasted compute cheaper or your developers’ time less valuable.

You already pay for your build infrastructure. You pay through hardware, cloud bills, duplicated execution, slow feedback, onboarding, and debugging environment differences.

Keeping the current system is an investment decision too. You are choosing to keep funding its limitations.

You do not need to migrate everything tomorrow. Start by measuring where the time and money go. Make dependencies explicit. Introduce shared caching. Move suitable actions to remote execution.

But start.

“But we’re not Google” is an excuse to avoid evaluating the investment. It is not an argument against making it.

## Can't have a blog post without AI

If you expect agents to produce more changes, you should expect to validate more changes.

That means more builds, more tests, and more environments requesting results. Repeatedly downloading toolchains and reconstructing build directories becomes an increasingly expensive habit.

Giving every agent a large machine and a fresh copy of the entire build environment is a costly default to scale.

Give agents a consistent way to request work from shared infrastructure. Let them reuse the results your developers and CI already produced. Give them access to the execution platforms their tasks require without making their sandboxes host every toolchain.

If you are investing in generating code faster, invest in validating it faster. Otherwise, you are increasing the pressure on a bottleneck you should already know exists.

**The place where you edit code should not dictate where it can be built.**

The future is remote. Start investing accordingly.

And _yes_, this blog [is built with Bazel.](https://github.com/LowkeyLab/bazel-repo)
