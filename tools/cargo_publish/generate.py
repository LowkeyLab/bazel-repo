#!/usr/bin/env python3
"""Generate declared Cargo inputs from the cargo_publish analysis contract.

This command never accesses the registry or discovers package files by scanning
checkout directories. All source, metadata, and provenance inputs are explicit.
"""

import argparse
import ast
import hashlib
import json
import re
import shutil
import sys
from pathlib import Path, PurePosixPath

import tomllib

CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
SEMVER = r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"


def read_toml(path):
    with Path(path).open("rb") as stream:
        return tomllib.load(stream)


def checked_path(path):
    parts = path.split("/")
    if (
        not path
        or path.startswith("/")
        or "\\" in path
        or any(p in ("", ".", "..") for p in parts)
    ):
        raise ValueError(
            f"package path must be relative and cannot escape the package: {path}"
        )
    if path in ("Cargo.toml", "Cargo.toml.orig", "Cargo.lock", ".cargo_vcs_info.json"):
        raise ValueError(f"package path conflicts with generated Cargo file: {path}")
    return PurePosixPath(path)


def unsupported_annotations(module):
    """Record literal root annotations, including platform-selected mutations."""
    result = {}
    harmless = {
        "crate",
        "version",
        "repositories",
        "triples",
        "gen_binaries",
        "crate_features",
        "tags",
        "visibility",
        "build_script_tags",
    }
    for node in ast.walk(ast.parse(Path(module).read_text())):
        if not isinstance(node, ast.Call) or not isinstance(node.func, ast.Attribute):
            continue
        kind = node.func.attr
        if kind not in ("annotation", "annotation_select", "override"):
            continue
        try:
            settings = {
                keyword.arg: ast.literal_eval(keyword.value)
                for keyword in node.keywords
            }
        except (ValueError, TypeError) as error:
            raise ValueError(
                "MODULE.bazel: ambiguous crate annotation/override configuration"
            ) from error
        if None in settings or not isinstance(settings.get("crate"), str):
            raise ValueError(
                "MODULE.bazel: ambiguous crate annotation/override identity"
            )
        repositories = settings.get("repositories", [])
        if repositories and "crates" not in repositories:
            continue
        version = settings.get("version") or "*"
        entry = result.setdefault(settings["crate"], {}).setdefault(version, {})
        translated = {
            key
            for key, value in settings.items()
            if key not in harmless
            and value
            and not (key == "gen_build_script" and value in ("auto", "on"))
        }
        if kind == "annotation_select":
            if "x86_64-unknown-linux-gnu" in settings.get("triples", []):
                entry.setdefault("selected", set()).update(translated)
        else:
            entry["normal"] = translated | (
                {"override"} if kind == "override" else set()
            )
    return result


def annotation_mutations(annotations, name, version):
    versions = annotations.get(name, {})
    wildcard = versions.get("*", {})
    exact = versions.get(version, {})
    # The pinned extension falls back to the wildcard ordinary annotation;
    # wildcard and exact platform selections compose.
    return sorted(
        exact.get("normal", wildcard.get("normal", set()))
        | wildcard.get("selected", set())
        | exact.get("selected", set())
    )


def dependency(metadata, root, lock):
    label = metadata["label"]
    if not metadata["external"]:
        raise ValueError(f"{label}: separate local dependency packages are unsupported")
    if metadata["compiler_settings"]:
        raise ValueError(
            f"{label}: unsupported compiler settings {metadata['compiler_settings']}"
        )
    if not metadata["manifest"] or not metadata["version"]:
        raise ValueError(
            f"{label}: ambiguous/missing Cargo manifest or version metadata"
        )
    upstream = read_toml(metadata["manifest"])
    package = upstream.get("package", {})
    name = package.get("name")
    version = metadata["version"]
    if not name or package.get("version") != version:
        raise ValueError(
            f"{label}: ambiguous Cargo package identity/version {name}@{version}"
        )
    expected_repository = f"rules_rs++crate+crates__{name}-{version}"
    if metadata.get("repository") != expected_repository:
        raise ValueError(
            f"{label}: unsupported or ambiguous configured repository provenance for {name}@{version}"
        )
    upstream_lib_name = upstream.get("lib", {}).get("name", name.replace("-", "_"))
    if upstream_lib_name != metadata["crate_name"]:
        raise ValueError(
            f"{label}: ambiguous library name {metadata['crate_name']} for package {name}"
        )
    declarations = []
    for kind in ("dependencies", "dev-dependencies", "build-dependencies"):
        for alias, item in root.get(kind, {}).items():
            item = {"version": item} if isinstance(item, str) else item
            if item.get("package", alias) == name:
                declarations.append(item)
    if not declarations:
        raise ValueError(
            f"{label}: {name} has no direct package identity in root Cargo manifest"
        )
    for item in declarations:
        if (
            any(key in item for key in ("git", "path", "registry", "workspace"))
            or "version" not in item
        ):
            raise ValueError(
                f"{label}: unsupported/ambiguous root dependency provenance for {name}"
            )
    locked = [
        item
        for item in lock.get("package", [])
        if item["name"] == name and item["version"] == version
    ]
    if (
        len(locked) != 1
        or locked[0].get("source") != CRATES_IO
        or not locked[0].get("checksum")
    ):
        raise ValueError(
            f"{label}: ambiguous or unsupported crates.io lock provenance for {name}@{version}"
        )
    features = sorted(set(metadata["features"]))
    known_features = set(upstream.get("features", {}))
    for kind in ("dependencies", "build-dependencies"):
        known_features.update(
            alias
            for alias, item in upstream.get(kind, {}).items()
            if isinstance(item, dict) and item.get("optional")
        )
    if set(features) - known_features:
        raise ValueError(
            f"{label}: ambiguous configured features {sorted(set(features) - known_features)} for {name}"
        )
    return {
        "target": label,
        "package": name,
        "version": version,
        "source": CRATES_IO,
        "checksum": locked[0]["checksum"],
        "features": features,
        "default_features": "default" in features,
    }


def generate(args):
    spec = json.loads(Path(args.spec).read_text())
    if spec.get("schema_version") != 1:
        raise ValueError("unsupported generation input schema_version")
    if spec.get("target") != "x86_64-unknown-linux-gnu":
        raise ValueError(
            f"{spec['library']['label']}: unsupported target platform {spec.get('target')}"
        )
    if spec["library"]["crate_type"] != "rlib":
        raise ValueError(
            f"{spec['library']['label']}: unsupported library crate type {spec['library']['crate_type']}"
        )
    if spec["library"]["edition"] != spec["package"]["edition"]:
        raise ValueError(
            f"{spec['library']['label']}: ambiguous library/package edition"
        )
    root = read_toml(args.root_manifest)
    lock = read_toml(args.root_lockfile)
    overrides = unsupported_annotations(args.module)
    package = spec["package"]
    for key in (
        "name",
        "version",
        "description",
        "license",
        "repository",
        "rust-version",
        "readme",
        "edition",
    ):
        if not isinstance(package.get(key), str) or not package[key]:
            raise ValueError(
                f"{spec['library']['label']}: missing package metadata {key}"
            )
    if spec["binaries"] or spec["tests"]:
        raise ValueError("binaries/tests are not yet supported in generation")
    if not re.fullmatch(r"[A-Za-z][A-Za-z0-9_-]*", package["name"]):
        raise ValueError(
            f"{spec['library']['label']}: malformed package name {package['name']}"
        )
    prerelease = package["version"].split("+", 1)[0].partition("-")[2]
    invalid_numeric_prerelease = any(
        part.isdecimal() and len(part) > 1 and part.startswith("0")
        for part in prerelease.split(".")
    )
    if not re.fullmatch(SEMVER, package["version"]) or invalid_numeric_prerelease:
        raise ValueError(
            f"{spec['library']['label']}: malformed package version {package['version']}"
        )
    if package["edition"] not in ("2015", "2018", "2021", "2024"):
        raise ValueError(
            f"{spec['library']['label']}: unsupported package edition {package['edition']}"
        )
    if not re.fullmatch(r"1\.[0-9]+(?:\.[0-9]+)?", package["rust-version"]):
        raise ValueError(
            f"{spec['library']['label']}: malformed rust-version {package['rust-version']}"
        )
    seen_closure = set()
    for metadata in spec["dependency_closure"]:
        identity = (metadata["label"], tuple(metadata["features"]), metadata["version"])
        if not metadata["external"] or identity in seen_closure:
            continue
        seen_closure.add(identity)
        # Only direct dependencies must occur in the root catalog. Transitive
        # source modifications still require explicit rejection below.
        upstream = read_toml(metadata["manifest"]) if metadata["manifest"] else {}
        name = upstream.get("package", {}).get("name")
        mutations = annotation_mutations(overrides, name, metadata["version"])
        if mutations:
            raise ValueError(
                f"{metadata['label']}: unsupported Bazel annotation/override for {name}: {mutations}"
            )
    dependencies = []
    aliases = {}
    for metadata in spec["library"]["dependencies"]:
        item = dependency(metadata, root, lock)
        alias = metadata["alias"]
        if alias in aliases:
            raise ValueError(
                f"{metadata['label']}: conflicting direct dependency alias {alias}"
            )
        aliases[alias] = item
        dependencies.append(dict(item, alias=alias, kind="normal"))
    dependencies.sort(key=lambda item: item["alias"])
    lines = ["[package]"]
    lines.extend(
        f"{key} = {json.dumps(value, ensure_ascii=False)}"
        for key, value in sorted(package.items())
    )
    lines.extend(
        [
            "build = false",
            "autolib = false",
            "autobins = false",
            "autotests = false",
            "autoexamples = false",
            "autobenches = false",
        ]
    )
    paths = sorted(item["path"] for item in spec["inputs"])
    lines.append("include = " + json.dumps(["Cargo.toml", *paths]))
    library = spec["library"]
    lines.extend(
        [
            "",
            "[lib]",
            f"name = {json.dumps(library['crate_name'])}",
            f"path = {json.dumps(library['path'])}",
            "",
            "[dependencies]",
        ]
    )
    for item in dependencies:
        values = [
            f"package = {json.dumps(item['package'])}",
            f"version = {json.dumps('=' + item['version'])}",
            "default-features = " + str(item["default_features"]).lower(),
            "features = " + json.dumps([f for f in item["features"] if f != "default"]),
        ]
        lines.append(json.dumps(item["alias"]) + " = { " + ", ".join(values) + " }")
    manifest = "\n".join(lines) + "\n"
    # Validate the entire declared inventory before writing output files.
    declared = {}
    for item in spec["inputs"]:
        path = str(checked_path(item["path"]))
        if path in declared:
            raise ValueError(f"conflicting package path: {path}")
        source = Path(item["source"])
        if not source.is_file():
            raise ValueError(f"missing declared package input: {path} from {source}")
        declared[path] = source
    for path in declared:
        if any(str(parent) in declared for parent in PurePosixPath(path).parents):
            raise ValueError(f"conflicting file/directory package path: {path}")
    for required in (package["readme"], library["path"]):
        if required not in declared:
            raise ValueError(f"missing required package path: {required}")
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    inventory = []
    for path, source in sorted(declared.items()):
        dest = output / path
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, dest)
        inventory.append(
            {
                "path": path,
                "sha256": hashlib.sha256(dest.read_bytes()).hexdigest(),
                "size": dest.stat().st_size,
            }
        )
    (output / "Cargo.toml").write_text(manifest)
    contract = {
        "schema_version": 1,
        "package": package,
        "target": spec["target"],
        "library": {
            key: library[key]
            for key in ("label", "crate_name", "crate_type", "edition", "path")
        },
        "binaries": [],
        "tests": [],
        "dependencies": dependencies,
        "inputs": inventory,
        "manifest_sha256": hashlib.sha256(manifest.encode()).hexdigest(),
        "provenance": {
            name: hashlib.sha256(Path(path).read_bytes()).hexdigest()
            for name, path in [
                ("root_manifest_sha256", args.root_manifest),
                ("root_lockfile_sha256", args.root_lockfile),
                ("module_sha256", args.module),
            ]
        },
    }
    Path(args.contract).write_text(
        json.dumps(contract, indent=2, sort_keys=True) + "\n"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in (
        "spec",
        "root-manifest",
        "root-lockfile",
        "module",
        "output",
        "contract",
    ):
        parser.add_argument("--" + name, required=True)
    args = parser.parse_args()
    try:
        generate(args)
    except (ValueError, OSError, KeyError, tomllib.TOMLDecodeError) as error:
        print(f"cargo_publish generation: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
