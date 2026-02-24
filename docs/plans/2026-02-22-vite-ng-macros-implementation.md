# vite_ng_application Symbolic Macro Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Create a `vite_ng_application` Bazel symbolic macro that encapsulates the boilerplate for building Angular apps with Vite + Analog.js, reducing BUILD files from ~67 lines to ~10 lines.

**Architecture:** A single symbolic macro defined with Bazel 9's `macro()` built-in. It internally creates `copy_to_bin`, `vite_bin.vite()`, and `js_run_devserver` targets. Lives in `angular/bzl/ng.bzl` alongside existing legacy macros.

**Tech Stack:** Bazel 9 symbolic macros, `@aspect_bazel_lib` (copy_to_bin), `@aspect_rules_js` (js_run_devserver), Vite binary from npm

---

### Task 1: Write the symbolic macro

**Files:**

- Modify: `angular/bzl/ng.bzl` (append new macro after existing ones)

**Step 1: Add the new loads at the top of ng.bzl**

Add these load statements alongside existing ones in `angular/bzl/ng.bzl`:

```starlark
load("@aspect_bazel_lib//lib:copy_to_bin.bzl", "copy_to_bin")
load("@aspect_rules_js//js:defs.bzl", "js_run_devserver")
load("@npm//angular:vite/package_json.bzl", vite_bin = "bin")
```

**Step 2: Write the implementation function and macro definition**

Append to `angular/bzl/ng.bzl`:

```starlark
# Core npm deps required by every Vite+Analog Angular app.
_VITE_NG_CORE_DEPS = [
    # keep-sorted start
    "//angular:node_modules/@analogjs/vite-plugin-angular",
    "//angular:node_modules/@angular/common",
    "//angular:node_modules/@angular/compiler",
    "//angular:node_modules/@angular/compiler-cli",
    "//angular:node_modules/@angular/core",
    "//angular:node_modules/@angular/platform-browser",
    "//angular:node_modules/rxjs",
    "//angular:node_modules/tslib",
    "//angular:node_modules/typescript",
    "//angular:node_modules/vite",
    # keep-sorted end
]

# Additional deps when tailwindcss = True.
_VITE_NG_TAILWIND_DEPS = [
    # keep-sorted start
    "//angular:node_modules/@tailwindcss/vite",
    "//angular:node_modules/daisyui",
    "//angular:node_modules/tailwindcss",
    # keep-sorted end
]

def _vite_ng_application_impl(name, visibility, srcs, deps, vite_config, tailwindcss):
    srcs_name = name + "_srcs"

    copy_to_bin(
        name = srcs_name,
        srcs = srcs,
    )

    all_deps = [srcs_name] + _VITE_NG_CORE_DEPS + deps
    if tailwindcss:
        all_deps = all_deps + _VITE_NG_TAILWIND_DEPS

    vite_bin.vite(
        name = name,
        visibility = visibility,
        srcs = all_deps,
        args = [
            "build",
            "--config",
            vite_config,
        ],
        chdir = native.package_name(),
        out_dirs = ["dist"],
    )

    js_run_devserver(
        name = name + "_dev",
        visibility = visibility,
        args = [
            "--config",
            vite_config,
        ],
        chdir = native.package_name(),
        command = "../../node_modules/.bin/vite",
        data = all_deps,
    )

vite_ng_application = macro(
    doc = "Builds an Angular application using Vite + Analog.js plugin.",
    implementation = _vite_ng_application_impl,
    attrs = {
        "srcs": attr.label_list(
            doc = "Source files to copy into the Bazel sandbox.",
        ),
        "deps": attr.label_list(
            default = [],
            doc = "App-specific npm dependencies.",
        ),
        "vite_config": attr.string(
            default = "vite.config.ts",
            configurable = False,
            doc = "Path to vite config relative to the package directory.",
        ),
        "tailwindcss": attr.bool(
            default = False,
            configurable = False,
            doc = "If True, adds Tailwind CSS v4 and DaisyUI dependencies.",
        ),
    },
)
```

**Step 3: Run format**

Run: `format`

**Step 4: Commit**

```bash
git add angular/bzl/ng.bzl
git commit -m "feat: add vite_ng_application symbolic macro"
```

---

### Task 2: Migrate nicknamer2-web BUILD.bazel to use the macro

**Files:**

- Modify: `angular/projects/nicknamer2-web/BUILD.bazel` (rewrite to use macro)

**Step 1: Replace BUILD.bazel contents**

Replace the entire `angular/projects/nicknamer2-web/BUILD.bazel` with:

```starlark
load("//angular/bzl:ng.bzl", "vite_ng_application")

package(default_visibility = ["//visibility:public"])

vite_ng_application(
    name = "nicknamer2-web",
    srcs = glob(
        [
            "src/**/*",
            "index.html",
            "vite.config.ts",
            "tsconfig.json",
            "tsconfig.app.json",
        ],
        exclude = ["dist/"],
    ),
    deps = [
        # keep-sorted start
        "//angular:node_modules/@angular/forms",
        "//angular:node_modules/@angular/router",
        "//angular:node_modules/@apollo/client",
        "//angular:node_modules/apollo-angular",
        "//angular:node_modules/graphql",
        "//angular:node_modules/zone.js",
        # keep-sorted end
    ],
    tailwindcss = True,
)
```

**Step 2: Build to verify**

Run: `aspect build //angular/projects/nicknamer2-web:nicknamer2-web`
Expected: Build succeeds, produces `dist/` with `index.html` and `assets/`

**Step 3: Verify dev server target exists**

Run: `aspect query //angular/projects/nicknamer2-web:nicknamer2-web_dev`
Expected: Target is found

**Step 4: Run full build**

Run: `aspect build //...`
Expected: All 253+ targets build successfully

**Step 5: Run full tests**

Run: `aspect test //...`
Expected: All tests pass

**Step 6: Run gazelle and format**

Run: `bazel run gazelle && format`

**Step 7: Commit**

```bash
git add angular/projects/nicknamer2-web/BUILD.bazel
git commit -m "refactor: migrate nicknamer2-web to vite_ng_application macro"
```

---

### Task 3: Finish the branch

Use `superpowers:finishing-a-development-branch` to push, create PR, or merge.
