"""Generate standalone Cargo publishing inputs from configured Rust targets."""

load("@rules_rust//rust:defs.bzl", "rust_common")
load("@rules_rust//rust/private:rustc.bzl", "ExtraExecRustcEnvInfo", "ExtraExecRustcFlagsInfo", "ExtraRustcEnvInfo", "ExtraRustcFlagsInfo", "PerCrateRustcFlagsInfo")
load(":providers.bzl", "CargoPublishInfo")
load(":runtime.bzl", "cargo_publish_runtime")

_CargoTargetInfo = provider(fields = ["metadata", "inputs", "closure"])

def _metadata_impl(target, ctx):
    if rust_common.crate_info not in target:
        return []
    crate = target[rust_common.crate_info]
    attrs = ctx.rule.attr
    aliases = {str(t[rust_common.crate_info].owner if rust_common.crate_info in t else t.label): value for t, value in getattr(attrs, "aliases", {}).items()}
    dependencies = []
    inputs = []
    closure = {}
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
    metadata = {
        "label": str(crate.owner),
        "crate_name": crate.name,
        "crate_type": crate.type,
        "edition": crate.edition,
        "version": getattr(attrs, "version", None),
        "features": sorted(getattr(attrs, "crate_features", [])),
        "external": crate.owner.workspace_name != "",
        "repository": crate.owner.workspace_name,
        "manifest": manifests[0].path if len(manifests) == 1 else None,
        "compiler_settings": settings,
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
    attr_aspects = ["deps", "proc_macro_deps"],
    toolchains = ["@rules_rust//rust:toolchain_type"],
)

def _package_path(path, label):
    if not path or path.startswith("/") or "\\" in path or any([part in ["", ".", ".."] for part in path.split("/")]):
        fail("{}: package path must be relative and cannot escape the package: {}".format(label, path))
    if path in ["Cargo.toml", "Cargo.toml.orig", "Cargo.lock", ".cargo_vcs_info.json"]:
        fail("{}: package path conflicts with generated Cargo file: {}".format(label, path))
    return path

def _files_impl(ctx):
    tc = ctx.toolchains["@rules_rust//rust:toolchain_type"]
    if tc.target_flag_value != "x86_64-unknown-linux-gnu":
        fail("{}: unsupported target platform {} (requires x86_64-unknown-linux-gnu)".format(ctx.label, tc.target_flag_value))
    if tc.extra_rustc_flags:
        fail("{}: unsupported toolchain extra_rustc_flags {}".format(ctx.label, tc.extra_rustc_flags))
    library = ctx.attr.library
    if library == None:
        fail("{}: a selected library is required in this publishing slice".format(ctx.label))
    if library.label.workspace_name:
        fail("{}: selected library {} must be local to the publishing repository".format(ctx.label, library.label))
    crate = library[rust_common.crate_info]
    metadata = library[_CargoTargetInfo].metadata
    if crate.type != "rlib" or crate.is_test:
        fail("{}: unsupported library crate type {}".format(library.label, crate.type))
    if metadata["features"]:
        fail("{}: unsupported publisher crate_features {}".format(library.label, metadata["features"]))

    # CrateInfo contains configured values; the aspect also records settings
    # absent from CrateInfo for the analysis rejection boundary.
    if metadata["compiler_settings"]:
        fail("{}: unsupported compiler settings {}".format(library.label, metadata["compiler_settings"]))
    if metadata["rustc_flags"]:
        fail("{}: unsupported publisher rustc_flags {}".format(library.label, metadata["rustc_flags"]))
    if metadata["link_deps"]:
        fail("{}: unsupported publisher link_deps {}".format(library.label, metadata["link_deps"]))
    if metadata["rustc_env"] or metadata["rustc_env_files"]:
        fail("{}: unsupported publisher rustc_env/rustc_env_files".format(library.label))
    entries = {}
    files = []
    prefix = ctx.label.package + "/" if ctx.label.package else ""
    for source in crate.srcs.to_list():
        if not source.is_source or source.is_directory or not source.short_path.startswith(prefix):
            fail("{}: unsupported generated/escaping source {}".format(library.label, source.short_path))
        path = _package_path(source.short_path[len(prefix):], library.label)
        entries[path] = source
        files.append(source)
    if crate.root not in files:
        fail("{}: crate_root {} is missing from declared srcs".format(library.label, crate.root.short_path))
    for target, path in ctx.attr.package_files.items():
        path = _package_path(path, target.label)
        assets = target[DefaultInfo].files.to_list()
        if len(assets) != 1 or assets[0].is_directory or not assets[0].is_source:
            fail("{}: package_files {} must select one declared source file".format(ctx.label, target.label))
        if path in entries:
            fail("{}: conflicting package path {} from {} and {}".format(ctx.label, path, entries[path].short_path, target.label))
        entries[path] = assets[0]
        files.append(assets[0])
    for path in entries:
        for parent in ["/".join(path.split("/")[:i]) for i in range(1, len(path.split("/")))]:
            if parent in entries:
                fail("{}: conflicting file/directory package path {}".format(ctx.label, path))
    for source in depset(transitive = [crate.compile_data, crate.data]).to_list():
        if source not in files:
            fail("{}: missing package_files mapping for compile_data {}".format(library.label, source.short_path))
    if ctx.attr.readme not in entries:
        fail("{}: missing package_files readme path {}".format(ctx.label, ctx.attr.readme))
    package = ctx.actions.declare_directory(ctx.label.name + ".package")
    contract = ctx.actions.declare_file(ctx.label.name + ".json")
    specification = ctx.actions.declare_file(ctx.label.name + ".analysis.json")
    metadata = dict(metadata, path = crate.root.short_path[len(prefix):])
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
            "edition": crate.edition,
        },
        "library": metadata,
        "binaries": [],
        "tests": [],
        "target": tc.target_flag_value,
        "inputs": [{"path": path, "source": entries[path].path} for path in sorted(entries)],
        "dependency_closure": [library[_CargoTargetInfo].closure[key] for key in sorted(library[_CargoTargetInfo].closure)],
    }
    ctx.actions.write(specification, json.encode(specification_data))
    args = ctx.actions.args()
    args.add_all(["--spec", specification, "--root-manifest", ctx.file.root_manifest, "--root-lockfile", ctx.file.root_lockfile, "--module", ctx.file.module, "--output", package, "--contract", contract], expand_directories = False)
    ctx.actions.run(
        executable = ctx.executable._generator,
        arguments = [args],
        inputs = depset(files + [specification, ctx.file.root_manifest, ctx.file.root_lockfile, ctx.file.module], transitive = [library[_CargoTargetInfo].inputs]),
        outputs = [package, contract],
        mnemonic = "CargoPublishFiles",
        progress_message = "Generating declared Cargo publishing files for %s" % ctx.label,
    )
    return [DefaultInfo(files = depset([package, contract]), runfiles = ctx.runfiles(files = [package, contract])), CargoPublishInfo(package = package, contract = contract)]

_cargo_publish_files = rule(
    implementation = _files_impl,
    attrs = {
        "library": attr.label(providers = [rust_common.crate_info], aspects = [_cargo_metadata]),
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
    """Declare inspectable <name>_files and explicit pinned-Cargo validation."""
    if binaries or tests:
        fail("{}: binaries/tests are not yet supported by cargo_publish".format(name))
    if library == None:
        fail("{}: cargo_publish requires a library or binary".format(name))
    _cargo_publish_files(name = name + "_files", library = library, **kwargs)
    runtime_kwargs = {key: kwargs[key] for key in ("tags", "visibility", "testonly", "target_compatible_with") if key in kwargs}
    cargo_publish_runtime(name = name, package = ":" + name + "_files", **runtime_kwargs)
