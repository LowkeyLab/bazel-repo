"""Bounded probes through the selected Rust toolchain."""

load("@llvm//toolchain:selects.bzl", "platform_resource_dir")
load("@rules_rust//rust:defs.bzl", "rust_common")

def _compiler_probe_impl(ctx):
    tc = ctx.toolchains["@rules_rust//rust:toolchain_type"]
    out = ctx.actions.declare_directory(ctx.label.name)
    ctx.actions.run_shell(
        inputs = depset(ctx.files.srcs, transitive = [tc.all_files]),
        outputs = [out],
        arguments = [tc.rustc.path, tc.sysroot, tc.target_flag_value, ctx.file.srcs.path, out.path, str(tc.coverage_supported)],
        command = """
set -u
compiler="$1"; sysroot="$2"; target="$3"; source="$4"; output="$5"
mkdir -p "$output"
printf 'coverage_supported=%s\\ntarget=%s\\n' "$6" "$target" > "$output/toolchain.txt"
"$compiler" -vV > "$output/rustc-vV.txt" 2>&1
for mode in plain instrument substitute; do
  flags=""
  case "$mode" in
    instrument) flags="-Cinstrument-coverage";;
    substitute) flags="-Cinstrument-coverage -Zno-profiler-runtime";;
  esac
  "$compiler" --sysroot "$sysroot" --target "$target" --crate-type cdylib --edition=2024 -Cpanic=abort $flags "$source" -o "$output/$mode.wasm" > "$output/$mode.log" 2>&1
  printf '%s\\n' "$?" > "$output/$mode.exit"
done
""",
        mnemonic = "WasmCompilerProbe",
    )
    return [DefaultInfo(files = depset([out]))]

compiler_probe = rule(
    implementation = _compiler_probe_impl,
    attrs = {"srcs": attr.label(allow_single_file = [".rs"], mandatory = True)},
    toolchains = ["@rules_rust//rust:toolchain_type"],
)

def _wasm_platform_impl(settings, attr):
    return {
        "//command_line_option:platforms": "@rules_rs//rs/platforms:wasm32-unknown-unknown",
        "@rules_rust//rust/toolchain/channel:channel": "nightly",
        "//command_line_option:extra_toolchains": ["@research_wasm_toolchains//..."],
        "//command_line_option:collect_code_coverage": False,
    }

_wasm_platform = transition(implementation = _wasm_platform_impl, inputs = [], outputs = ["//command_line_option:platforms", "@rules_rust//rust/toolchain/channel:channel", "//command_line_option:extra_toolchains", "//command_line_option:collect_code_coverage"])

def _runtime_impl(ctx):
    tc = ctx.attr.compiler[0][platform_common.ToolchainInfo]
    out = ctx.actions.declare_directory(ctx.label.name)
    root = ctx.file.minicov_root.dirname
    bindgen = ctx.attr.wasm_bindgen[0]
    crate = bindgen[rust_common.crate_info]
    dep = bindgen[rust_common.dep_info]
    dep_paths = ["-Ldependency=" + f.dirname for f in dep.transitive_crate_outputs.to_list()]
    ctx.actions.run_shell(
        inputs = depset(ctx.files.srcs + ctx.files.minicov_sources + ctx.files.clang_resources, transitive = [tc.all_files, depset([crate.output]), dep.transitive_crate_outputs, dep.transitive_proc_macro_data]),
        tools = [ctx.executable.clang, ctx.executable.ar],
        outputs = [out],
        arguments = [tc.rustc.path, tc.sysroot, root, ctx.executable.clang.path, ctx.executable.ar.path, out.path, ctx.file.crate_root.path, crate.output.path] + dep_paths,
        command = """
set -eu
compiler="$1"; sysroot="$2"; runtime="$3"; clang="$4"; ar="$5"; output="$6"; source="$7"; bindgen="$8"
shift 8
mkdir -p "$output/objects"
sha256sum "$source" "${source%/*}/subject.rs" > "$output/source-digests.sha256"
"$compiler" -vV > "$output/rustc-vV.txt"
"$clang" --version > "$output/clang-version.txt"
"$ar" --version > "$output/ar-version.txt"
sha256sum "$compiler" "$clang" "$ar" > "$output/compiler-tool-digests.sha256"
for file in InstrProfiling InstrProfilingBuffer InstrProfilingInternal InstrProfilingMerge InstrProfilingMergeFile InstrProfilingPlatformLinux InstrProfilingPlatformWindows InstrProfilingWriter InstrProfilingValue InstrProfilingVersionVar; do
  "$clang" --target=wasm32-unknown-unknown -nostdlibinc -fno-stack-protector -fno-profile-generate -fno-profile-use -fno-profile-instr-generate -fno-profile-instr-use -fno-coverage-mapping -DCOMPILER_RT_HAS_ATOMICS=1 -DCOMPILER_RT_PROFILE_BAREMETAL=1 -c "$runtime/c/$file.c" -o "$output/objects/$file.o"
done
"$ar" rcs "$output/libllvm_profiler_runtime.a" "$output"/objects/*.o
"$compiler" --sysroot "$sysroot" --target wasm32-unknown-unknown --edition=2021 --crate-name minicov --crate-type rlib "$runtime/src/lib.rs" -o "$output/libminicov.rlib"
"$compiler" --sysroot "$sysroot" --target wasm32-unknown-unknown --edition=2024 --crate-type cdylib --crate-name browser_fixture -Cpanic=abort -Copt-level=0 -Cinstrument-coverage -Zno-profiler-runtime -Clink-arg=--no-gc-sections -Clink-arg=--emit-relocs --emit=link,llvm-ir --extern "minicov=$output/libminicov.rlib" --extern "wasm_bindgen=$bindgen" "$@" -L "native=$output" -l static=llvm_profiler_runtime "$source" -o "$output/fixture.wasm"
set +e
"$clang" --target=x86_64-unknown-linux-gnu -c -x ir "$output/fixture.ll" -o "$output/fixture-ir.o" > "$output/ir-object.log" 2>&1
printf '%s\\n' "$?" > "$output/ir-object.exit"
""",
        env = {"CARGO_PKG_NAME": "wasm-coverage-fixture", "CARGO_PKG_VERSION": "0.0.0", "CARGO_MANIFEST_DIR": "tools/research_wasm"},
        mnemonic = "WasmRuntimeFixture",
    )
    return [DefaultInfo(files = depset([out]))]

runtime_fixture = rule(
    implementation = _runtime_impl,
    attrs = {
        "srcs": attr.label_list(allow_files = True),
        "crate_root": attr.label(allow_single_file = True),
        "wasm_bindgen": attr.label(default = "@crates//:wasm-bindgen", cfg = _wasm_platform),
        "compiler": attr.label(default = "@research_wasm_toolchains//rustc:default_linux_x86_64_nightly_2026_08_15_rust_toolchain", cfg = _wasm_platform),
        "_allowlist_function_transition": attr.label(default = "@bazel_tools//tools/allowlists/function_transition_allowlist"),
        "minicov_root": attr.label(allow_single_file = True, default = "@research_minicov//:Cargo.toml"),
        "minicov_sources": attr.label(default = "@research_minicov//:sources"),
        "clang_resources": attr.label(cfg = "exec", default = platform_resource_dir("linux", "x86_64")),
        "clang": attr.label(executable = True, allow_files = True, cfg = "exec", default = "@llvm//tools:clang"),
        "ar": attr.label(executable = True, allow_files = True, cfg = "exec", default = "@llvm//tools:llvm-ar"),
    },
    toolchains = ["@rules_rust//rust:toolchain_type"],
)

def _web_impl(ctx):
    out = ctx.actions.declare_directory(ctx.label.name)
    ctx.actions.run_shell(
        inputs = ctx.files.src,
        tools = [ctx.attr.bindgen[DefaultInfo].files_to_run],
        outputs = [out],
        arguments = [ctx.executable.bindgen.path, ctx.file.src.path, out.path],
        command = 'mkdir -p "$3"; "$1" --version > "$3/version.txt"; sha256sum "$1" > "$3/bindgen.sha256"; "$1" --target web --out-dir "$3" --out-name fixture "$2/browser_fixture.wasm"',
        mnemonic = "WasmBindgenWebFixture",
    )
    return [DefaultInfo(files = depset([out]))]

web_fixture = rule(
    implementation = _web_impl,
    attrs = {
        "src": attr.label(allow_single_file = True),
        "bindgen": attr.label(executable = True, cfg = "exec", default = "@crates//:wasm-bindgen-cli__wasm-bindgen"),
    },
)
