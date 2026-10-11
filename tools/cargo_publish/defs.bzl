"""Generate standalone Cargo publishing inputs from configured Rust targets."""

load("@rules_rust//rust:defs.bzl", "rust_common")
load("@rules_rust//rust/private:rustc.bzl", "ExtraExecRustcEnvInfo", "ExtraExecRustcFlagsInfo", "ExtraRustcEnvInfo", "ExtraRustcFlagsInfo", "PerCrateRustcFlagsInfo")
load("@rules_rust//rust/private:stamp.bzl", "is_stamping_enabled")

CargoPublishInfo = provider(
    doc = "Declared package tree and versioned generation contract for validation.",
    fields = {"package": "Staged tree artifact", "contract": "Generation contract JSON"},
)

_CargoTargetInfo = provider(fields = ["metadata", "inputs", "closure"])

def _metadata_impl(target, ctx):
    if rust_common.crate_info not in target:
        return []
    crate = target[rust_common.crate_info]
    attrs = ctx.rule.attr
    original = getattr(attrs, "crate", None)
    aliases = {str(t[rust_common.crate_info].owner if rust_common.crate_info in t else t.label): value for t, value in crate.aliases.items()}
    aliases.update({str(t[rust_common.crate_info].owner if rust_common.crate_info in t else t.label): value for t, value in getattr(attrs, "aliases", {}).items()})
    dependencies = []
    inputs = []
    closure = {}
    if original and _CargoTargetInfo in original:
        inputs.append(original[_CargoTargetInfo].inputs)
        closure.update(original[_CargoTargetInfo].closure)
    for group in ["deps", "proc_macro_deps"]:
        for dep in getattr(attrs, group, []):
            if _CargoTargetInfo in dep:
                info = dep[_CargoTargetInfo]

                # rules_rust duplicates declared deps into exec configuration,
                # then selects proc macros from that copy only.
                is_proc_macro = info.metadata["crate_type"] == "proc-macro"
                if is_proc_macro != (group == "proc_macro_deps"):
                    continue
                alias = aliases.get(info.metadata["label"], info.metadata["crate_name"])
                dependency_metadata = {key: value for key, value in info.metadata.items() if key != "dependencies"}
                dependencies.append(dict(dependency_metadata, alias = alias))
                inputs.append(info.inputs)
                closure.update(info.closure)
            elif crate.owner.workspace_name == "" and group == "deps":
                fail("{}: unsupported dependency {} (build script/native input or missing Rust metadata)".format(target.label, dep.label))
    manifests = [f for f in crate.compile_data.to_list() if f.basename == "Cargo.toml"]
    settings = {}
    for setting, provider_type, field in [
        ("_extra_rustc_flags", ExtraRustcFlagsInfo, "extra_rustc_flags"),
        ("_extra_rustc_flag", ExtraRustcFlagsInfo, "extra_rustc_flags"),
        ("_extra_exec_rustc_flags", ExtraExecRustcFlagsInfo, "extra_exec_rustc_flags"),
        ("_extra_exec_rustc_flag", ExtraExecRustcFlagsInfo, "extra_exec_rustc_flags"),
        ("_per_crate_rustc_flag", PerCrateRustcFlagsInfo, "per_crate_rustc_flags"),
        ("_extra_rustc_env", ExtraRustcEnvInfo, "extra_rustc_env"),
        ("_extra_exec_rustc_env", ExtraExecRustcEnvInfo, "extra_exec_rustc_env"),
    ]:
        value = getattr(attrs, setting, None)
        if value != None and provider_type in value:
            configured = getattr(value[provider_type], field)
            if configured:
                settings[setting] = configured
    tc = ctx.toolchains["@rules_rust//rust:toolchain_type"]
    for setting in ["extra_rustc_flags", "extra_exec_rustc_flags"]:
        if getattr(tc, setting):
            settings[setting] = getattr(tc, setting)
    if tc.extra_rustc_flags_for_crate_types.get(crate.type):
        settings["extra_rustc_flags_for_crate_types"] = tc.extra_rustc_flags_for_crate_types[crate.type]
    publisher_settings = {}
    for setting in ["alwayslink", "linker_script"]:
        value = getattr(attrs, setting, None)
        if value:
            publisher_settings[setting] = str(value)
    if getattr(attrs, "link_std_dylib", False) or tc._link_std_dylib:
        publisher_settings["link_std_dylib"] = True
    if is_stamping_enabled(attrs):
        publisher_settings["stamp"] = True
    metadata = {
        "label": str(crate.owner),
        "crate_name": crate.name,
        "crate_type": crate.type,
        "edition": crate.edition,
        "target": tc.target_flag_value,
        "is_test": crate.is_test,
        "crate": str(original.label) if original else None,
        "version": getattr(attrs, "version", None),
        "features": sorted(getattr(attrs, "crate_features", [])),
        "external": crate.owner.workspace_name != "",
        "repository": crate.owner.workspace_name,
        "manifest": manifests[0].path if len(manifests) == 1 else None,
        "compiler_settings": settings,
        "publisher_settings": publisher_settings,
        "rustc_flags": getattr(attrs, "rustc_flags", []),
        "rustc_env": getattr(attrs, "rustc_env", {}),
        "rustc_env_files": [str(t.label) for t in getattr(attrs, "rustc_env_files", [])],
        "link_deps": [str(t.label) for t in getattr(attrs, "link_deps", [])],
        "dependencies": dependencies,
    }
    closure[json.encode({key: value for key, value in metadata.items() if key != "dependencies"})] = metadata
    return [_CargoTargetInfo(metadata = metadata, inputs = depset(manifests, transitive = inputs), closure = closure)]

_cargo_metadata = aspect(
    implementation = _metadata_impl,
    attr_aspects = ["deps", "proc_macro_deps", "crate"],
    toolchains = ["@rules_rust//rust:toolchain_type"],
)

def _package_path(path, label):
    if not path or path.startswith("/") or "\\" in path or any([part in ["", ".", ".."] for part in path.split("/")]):
        fail("{}: package path must be relative and cannot escape the package: {}".format(label, path))
    if path in ["Cargo.toml", "Cargo.toml.orig", "Cargo.lock", ".cargo_vcs_info.json"]:
        fail("{}: package path conflicts with generated Cargo file: {}".format(label, path))
    return path

def _selected_metadata(target, expected_type, prefix, entries):
    if target.label.workspace_name:
        fail("{}: selected publisher must be local to the publishing repository".format(target.label))
    crate = target[rust_common.crate_info]
    metadata = target[_CargoTargetInfo].metadata
    kind = "library" if expected_type == "rlib" else "binary"
    if crate.type != expected_type or crate.is_test:
        fail("{}: unsupported {} crate type {}".format(target.label, kind, crate.type))
    if metadata["target"] != "x86_64-unknown-linux-gnu":
        fail("{}: unsupported target platform {}".format(target.label, metadata["target"]))
    for setting, value in metadata["publisher_settings"].items():
        fail("{}: unsupported publisher {} {}".format(target.label, setting, value))
    if metadata["features"]:
        fail("{}: unsupported publisher crate_features {}".format(target.label, metadata["features"]))
    if metadata["compiler_settings"]:
        fail("{}: unsupported compiler settings {}".format(target.label, metadata["compiler_settings"]))
    if metadata["rustc_flags"]:
        fail("{}: unsupported publisher rustc_flags {}".format(target.label, metadata["rustc_flags"]))
    if metadata["link_deps"]:
        fail("{}: unsupported publisher link_deps {}".format(target.label, metadata["link_deps"]))
    if metadata["rustc_env"] or metadata["rustc_env_files"]:
        fail("{}: unsupported publisher rustc_env/rustc_env_files".format(target.label))
    files = crate.srcs.to_list()
    for source in files:
        if not source.is_source or source.is_directory or not source.short_path.startswith(prefix):
            fail("{}: unsupported generated/escaping source {}".format(target.label, source.short_path))
        path = _package_path(source.short_path[len(prefix):], target.label)
        if path in entries and entries[path] != source:
            fail("{}: conflicting package path {}".format(target.label, path))
        entries[path] = source
    if crate.root not in files:
        fail("{}: crate_root {} is missing from declared srcs".format(target.label, crate.root.short_path))
    return dict(metadata, path = crate.root.short_path[len(prefix):])

def _files_impl(ctx):
    tc = ctx.toolchains["@rules_rust//rust:toolchain_type"]
    if tc.target_flag_value != "x86_64-unknown-linux-gnu":
        fail("{}: unsupported target platform {} (requires x86_64-unknown-linux-gnu)".format(ctx.label, tc.target_flag_value))
    selected = ([ctx.attr.library] if ctx.attr.library else []) + ctx.attr.binaries.keys()
    if not selected:
        fail("{}: cargo_publish requires a library or binary".format(ctx.label))
    entries = {}
    prefix = ctx.label.package + "/" if ctx.label.package else ""
    library = _selected_metadata(ctx.attr.library, "rlib", prefix, entries) if ctx.attr.library else None
    named_binaries = {}
    for target, name in ctx.attr.binaries.items():
        if name in named_binaries:
            fail("{}: conflicting Cargo binary name {}".format(target.label, name))
        named_binaries[name] = dict(_selected_metadata(target, "bin", prefix, entries), name = name)
    binaries = [named_binaries[name] for name in sorted(named_binaries)]
    tests = []
    for target in ctx.attr.tests:
        metadata = target[_CargoTargetInfo].metadata
        if target.label.workspace_name or not metadata["is_test"]:
            fail("{}: selected test must be a local Rust test target".format(target.label))
        if metadata["target"] != "x86_64-unknown-linux-gnu":
            fail("{}: unsupported target platform {}".format(target.label, metadata["target"]))
        tests.append(metadata)
    for target, path in ctx.attr.package_files.items():
        path = _package_path(path, target.label)
        assets = target[DefaultInfo].files.to_list()
        if len(assets) != 1 or assets[0].is_directory or not assets[0].is_source:
            fail("{}: package_files {} must select one declared source file".format(ctx.label, target.label))
        if path in entries:
            fail("{}: conflicting package path {} from {} and {}".format(ctx.label, path, entries[path].short_path, target.label))
        entries[path] = assets[0]
    for path in entries:
        for parent in ["/".join(path.split("/")[:i]) for i in range(1, len(path.split("/")))]:
            if parent in entries:
                fail("{}: conflicting file/directory package path {}".format(ctx.label, path))
    files = entries.values()
    inputs = []
    closure = {}
    for target in selected:
        crate = target[rust_common.crate_info]
        for source in depset(transitive = [crate.compile_data, crate.data]).to_list():
            if source not in files:
                fail("{}: missing package_files mapping for compile_data {}".format(target.label, source.short_path))
        inputs.append(target[_CargoTargetInfo].inputs)
        closure.update(target[_CargoTargetInfo].closure)
    for target in ctx.attr.tests:
        inputs.append(target[_CargoTargetInfo].inputs)
        closure.update(target[_CargoTargetInfo].closure)
    if ctx.attr.readme not in entries:
        fail("{}: missing package_files readme path {}".format(ctx.label, ctx.attr.readme))
    package = ctx.actions.declare_directory(ctx.label.name + ".package")
    contract = ctx.actions.declare_file(ctx.label.name + ".json")
    specification = ctx.actions.declare_file(ctx.label.name + ".analysis.json")
    specification_data = {
        "schema_version": 1,
        "package": {
            "name": ctx.attr.package_name,
            "version": ctx.attr.version,
            "description": ctx.attr.description,
            "license": ctx.attr.license,
            "repository": ctx.attr.repository,
            "rust-version": ctx.attr.rust_version,
            "readme": ctx.attr.readme,
            "edition": library["edition"] if library else binaries[0]["edition"],
        },
        "library": library,
        "binaries": binaries,
        "tests": tests,
        "target": tc.target_flag_value,
        "inputs": [{"path": path, "source": entries[path].path} for path in sorted(entries)],
        "dependency_closure": [closure[key] for key in sorted(closure)],
    }
    ctx.actions.write(specification, json.encode(specification_data))
    args = ctx.actions.args()
    args.add_all(["--spec", specification, "--root-manifest", ctx.file.root_manifest, "--root-lockfile", ctx.file.root_lockfile, "--module", ctx.file.module, "--output", package, "--contract", contract], expand_directories = False)
    ctx.actions.run(
        executable = ctx.executable._generator,
        arguments = [args],
        inputs = depset(files + [specification, ctx.file.root_manifest, ctx.file.root_lockfile, ctx.file.module], transitive = inputs),
        outputs = [package, contract],
        mnemonic = "CargoPublishFiles",
        progress_message = "Generating declared Cargo publishing files for %s" % ctx.label,
    )
    return [DefaultInfo(files = depset([package, contract]), runfiles = ctx.runfiles(files = [package, contract])), CargoPublishInfo(package = package, contract = contract)]

_cargo_publish_files = rule(
    implementation = _files_impl,
    attrs = {
        "library": attr.label(providers = [rust_common.crate_info], aspects = [_cargo_metadata]),
        "binaries": attr.label_keyed_string_dict(providers = [rust_common.crate_info], aspects = [_cargo_metadata]),
        "tests": attr.label_list(providers = [rust_common.crate_info], aspects = [_cargo_metadata]),
        "package_files": attr.label_keyed_string_dict(allow_files = True),
        "package_name": attr.string(mandatory = True),
        "version": attr.string(mandatory = True),
        "description": attr.string(mandatory = True),
        "license": attr.string(mandatory = True),
        "repository": attr.string(mandatory = True),
        "rust_version": attr.string(mandatory = True),
        "readme": attr.string(mandatory = True),
        "root_manifest": attr.label(allow_single_file = True, default = Label("//:Cargo.toml")),
        "root_lockfile": attr.label(allow_single_file = True, default = Label("//:Cargo.lock")),
        "module": attr.label(allow_single_file = True, default = Label("//:MODULE.bazel")),
        "_generator": attr.label(default = Label("//tools/cargo_publish:generate"), executable = True, cfg = "exec"),
    },
    toolchains = ["@rules_rust//rust:toolchain_type"],
)

def cargo_publish(name, library = None, binaries = {}, tests = [], **kwargs):
    """Declare inspectable <name>_files inputs; validation is a later slice."""
    if library == None and not binaries:
        fail("{}: cargo_publish requires a library or binary".format(name))
    testonly = kwargs.pop("testonly", bool(tests))
    _cargo_publish_files(name = name + "_files", library = library, binaries = binaries, tests = tests, testonly = testonly, **kwargs)
