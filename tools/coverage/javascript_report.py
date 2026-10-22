"""Validate JavaScript collection and merge native manifest baselines."""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
import zipfile
from pathlib import Path
from typing import TypedDict
from urllib.parse import unquote, urlparse


class AngularProject(TypedDict):
    target_prefix: str
    runner_sources: set[str]


EXTENSIONS = {".js", ".mjs", ".cjs", ".jsx", ".ts", ".mts", ".cts", ".tsx", ".astro"}


def records(path: Path, validate_all: bool = False) -> dict[str, list[str]]:
    text = path.read_text()
    parts = text.split("end_of_record")
    if len(parts) == 1 or parts.pop().strip():
        raise ValueError(f"Missing or incomplete JavaScript LCOV: {path}")
    result = {}
    for part in parts:
        lines = part.strip().splitlines()
        names = [line[3:] for line in lines if line.startswith("SF:")]
        if len(names) != 1 or names[0] in result:
            raise ValueError(f"Invalid JavaScript source record: {path}")
        name = names[0]
        if validate_all or Path(name).suffix in EXTENSIONS:
            counts = {}
            branches = {}
            for line in lines:
                if line.startswith("DA:"):
                    match = re.fullmatch(r"DA:([1-9][0-9]*),(\d+)(?:,[^,]+)?", line)
                    if match is None or int(match[1]) in counts:
                        raise ValueError(
                            f"Invalid JavaScript line record: {path}: {name}"
                        )
                    counts[int(match[1])] = int(match[2])
                if line.startswith("BRDA:"):
                    branch = re.fullmatch(r"BRDA:([1-9][0-9]*,\d+,\d+),(-|\d+)", line)
                    if branch is None or branch[1] in branches:
                        raise ValueError(
                            f"Invalid JavaScript branch record: {path}: {name}"
                        )
                    branches[branch[1]] = 0 if branch[2] == "-" else int(branch[2])
                if ":" not in line or line.split(":", 1)[0] not in {
                    "TN",
                    "SF",
                    "VER",
                    "FN",
                    "FNDA",
                    "FNF",
                    "FNH",
                    "DA",
                    "LF",
                    "LH",
                    "BRDA",
                    "BRF",
                    "BRH",
                }:
                    raise ValueError(f"Invalid JavaScript LCOV field: {path}")
                if line.startswith("FN:") and not re.fullmatch(
                    r"FN:[0-9]+,(?:[0-9]+,)?.+", line
                ):
                    raise ValueError(f"Invalid JavaScript function record: {path}")
                if line.startswith("FNDA:") and not re.fullmatch(
                    r"FNDA:[0-9]+,.+", line
                ):
                    raise ValueError(f"Invalid JavaScript function record: {path}")
            found = [line for line in lines if line.startswith("LF:")]
            hit = [line for line in lines if line.startswith("LH:")]
            if found != [f"LF:{len(counts)}"] or hit != [
                f"LH:{sum(count > 0 for count in counts.values())}"
            ]:
                raise ValueError(
                    f"Incomplete JavaScript executable records: {path}: {name}"
                )
            function_found = [line for line in lines if line.startswith("FNF:")]
            function_hit = [line for line in lines if line.startswith("FNH:")]
            definitions = [line for line in lines if line.startswith("FN:")]
            calls = [
                int(line[5:].split(",", 1)[0])
                for line in lines
                if line.startswith("FNDA:")
            ]
            if (definitions or calls or function_found or function_hit) and (
                function_found != [f"FNF:{len(definitions)}"]
                or len(calls) != len(definitions)
                or function_hit != [f"FNH:{sum(count > 0 for count in calls)}"]
            ):
                raise ValueError(f"Incomplete JavaScript function coverage: {path}")
            branch_found = [line for line in lines if line.startswith("BRF:")]
            branch_hit = [line for line in lines if line.startswith("BRH:")]
            if (branches or branch_found or branch_hit) and (
                branch_found != [f"BRF:{len(branches)}"]
                or branch_hit
                != [f"BRH:{sum(count > 0 for count in branches.values())}"]
            ):
                raise ValueError(
                    f"Incomplete JavaScript branch coverage: {path}: {name}"
                )
        result[name] = lines
    return result


def has_hits(lines: list[str]) -> bool:
    for line in lines:
        if line.startswith("DA:") and int(line.split(",")[1]) > 0:
            return True
        if line.startswith("FNDA:") and int(line[5:].split(",", 1)[0]) > 0:
            return True
        if line.startswith("BRDA:"):
            count = line.rsplit(",", 1)[1]
            if count != "-" and int(count) > 0:
                return True
    return False


def local_path(file: dict[str, str]) -> Path:
    uri = urlparse(file["uri"])
    if uri.scheme != "file" or uri.netloc:
        raise ValueError("Required JavaScript artifact is not a local file URI.")
    return Path(unquote(uri.path))


def browser_reports(
    outputs: dict[str, dict[str, str]],
    browser_directory: Path,
    sources: set[str],
    root: Path,
    label: str,
) -> tuple[list[Path], dict[str, str]]:
    """Validate browser artifacts before accepting their original-source records."""
    observed: dict[str, str] = {}
    browser_reports: list[Path] = []
    browser_directory.mkdir()
    files = {}
    if "test.outputs__outputs.zip" in outputs:
        archive_path = local_path(outputs["test.outputs__outputs.zip"])
        shutil.copyfile(archive_path, browser_directory / "outputs.zip")
        with zipfile.ZipFile(archive_path) as archive:
            files = {
                name: archive.read(name)
                for name in archive.namelist()
                if name.startswith("browser-coverage/") and not name.endswith("/")
            }
    else:
        files = {
            name[name.index("browser-coverage/") :]: local_path(file).read_bytes()
            for name, file in outputs.items()
            if "browser-coverage/" in name
        }
        for name, content in files.items():
            if ".." in Path(name).parts:
                raise ValueError(f"Invalid browser artifact path: {name}")
            destination = browser_directory / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(content)
    if not files:
        raise ValueError(f"Missing required browser coverage: {label}")
    receipts = sorted(name for name in files if name.endswith("/receipt.json"))
    if not receipts:
        raise ValueError(f"Missing required browser coverage receipts: {label}")
    pages = json.loads(files["browser-coverage/pages.json"])
    if (
        not isinstance(pages, list)
        or not pages
        or any(
            not isinstance(page, str) or not re.fullmatch(r"[A-Za-z0-9-]+", page)
            for page in pages
        )
        or len(set(pages)) != len(pages)
        or sorted(f"browser-coverage/{page}/receipt.json" for page in pages) != receipts
    ):
        raise ValueError(f"Incomplete required browser page outputs: {label}")
    for index, receipt_name in enumerate(receipts):
        prefix = receipt_name.removesuffix("receipt.json")
        data = json.loads(files[receipt_name])
        if any(
            hashlib.sha256(files[prefix + filename]).hexdigest() != data.get(key)
            for filename, key in [
                ("profile.json", "profile_sha256"),
                ("report.lcov", "report_sha256"),
            ]
        ):
            raise ValueError(f"Corrupt required browser output: {label}")
        raw = json.loads(files[prefix + "profile.json"])
        if (
            data.get("schema_version") != 1
            or not isinstance(raw, list)
            or not isinstance(data.get("scripts"), list)
            or not isinstance(data.get("sources"), list)
        ):
            raise ValueError(f"Invalid browser coverage receipt: {label}")
        profiles = [
            entry
            for entry in raw
            if urlparse(entry["url"]).hostname == "127.0.0.1"
            and urlparse(entry["url"]).path.endswith(".js")
        ]
        if len(profiles) != len(data["scripts"]):
            raise ValueError(f"Incomplete browser coverage inputs: {label}")
        verified = set()
        for script_index, (profile, script) in enumerate(
            zip(profiles, data["scripts"], strict=True)
        ):
            source = files[prefix + f"{script_index}.js"].decode()
            map_bytes = files[prefix + f"{script_index}.map"]
            if hashlib.sha256(map_bytes).hexdigest() != script.get("map_sha256"):
                raise ValueError(f"Corrupt required browser source map: {label}")
            source_map = json.loads(map_bytes)
            if (
                profile.get("source") != source
                or source_map.get("version") != 3
                or not isinstance(source_map.get("mappings"), str)
                or source_map.get("sources") != script["sources"]
                or source_map.get("sourcesContent") != script["sourcesContent"]
            ):
                raise ValueError(f"Invalid required browser source map: {label}")
            for function in profile["functions"]:
                if not function.get("ranges"):
                    raise ValueError(f"Invalid browser function ranges: {label}")
                for interval in function["ranges"]:
                    if (
                        any(
                            type(interval.get(key)) is not int
                            for key in ("startOffset", "endOffset", "count")
                        )
                        or not 0
                        <= interval["startOffset"]
                        <= interval["endOffset"]
                        <= len(source.encode("utf-16-le")) // 2
                        or interval["count"] < 0
                    ):
                        raise ValueError(f"Invalid browser coverage range: {label}")
            for original, content in zip(
                script["sources"], script["sourcesContent"], strict=True
            ):
                mapped = os.path.normpath(
                    os.path.join(
                        os.path.dirname(script["script"]),
                        source_map.get("sourceRoot", ""),
                        original,
                    )
                )
                candidates = [
                    name
                    for name in sources
                    if mapped == name or mapped.endswith("/" + name)
                ]
                if len(candidates) > 1:
                    raise ValueError(f"Ambiguous browser source: {mapped}")
                if candidates and (root / candidates[0]).read_text() == content:
                    verified.add(candidates[0])
        saved = browser_directory / f"{index}.lcov"
        saved.write_bytes(files[prefix + "report.lcov"])
        measured_browser = records(saved, validate_all=True) if data["sources"] else {}
        if set(measured_browser) != set(data["sources"]) or (
            not data["sources"] and saved.read_text().strip()
        ):
            raise ValueError(f"Incomplete browser report: {label}")
        canonical = {}
        for original, lines in measured_browser.items():
            candidates = [
                name
                for name in verified
                if original == name or original.endswith("/" + name)
            ]
            if len(candidates) > 1:
                raise ValueError(f"Ambiguous browser report source: {original}")
            if not candidates:
                continue
            name = candidates[0]
            if name in canonical:
                raise ValueError(f"Duplicate canonical browser source: {name}")
            canonical[name] = [
                "SF:" + name if line.startswith("SF:") else line for line in lines
            ]
            observed[name] = str(saved)
        supported_browser = browser_directory / f"{index}-supported.lcov"
        supported_browser.write_text(
            "".join(
                "\n".join(lines) + "\nend_of_record\n" for lines in canonical.values()
            )
        )
        browser_reports.append(supported_browser)
    return browser_reports, observed


def main() -> None:
    os.umask(0o077)
    parser = argparse.ArgumentParser()
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--events", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--collector", required=True)
    parser.add_argument("--merger", required=True)
    args = parser.parse_args()
    root = Path(os.environ.get("BUILD_WORKSPACE_DIRECTORY", os.getcwd())).resolve()
    os.chdir(root)
    inventory = json.loads(args.inventory.read_text())
    sources = {
        item["path"]: item
        for item in inventory["eligible"]
        if item["language"] in {"javascript", "browser"}
    }
    excluded = {item["path"] for item in inventory["excluded"]}
    evidence = args.evidence.resolve()
    evidence.mkdir(parents=True, exist_ok=True)
    run_directory = Path(tempfile.mkdtemp(prefix="run-", dir=evidence))
    shutil.copyfile(args.inventory, run_directory / "inventory.json")
    event_text = re.sub(
        r"(?i)(x-buildbuddy-api-key[=:])[^\s\"'\\]+",
        r"\1<REDACTED>",
        args.events.read_text(),
    )
    (run_directory / "events.json").write_text(event_text)
    angular_file = root / "angular/angular.json"
    projects: dict[str, AngularProject] = {}
    if angular_file.exists():
        for name, project in (
            json.loads(angular_file.read_text()).get("projects", {}).items()
        ):
            test = project.get("architect", {}).get("test", {})
            if test.get("builder") == "@angular/build:unit-test":
                prefix = (
                    "angular/"
                    + project.get("sourceRoot", project["root"] + "/src").rstrip("/")
                    + "/"
                )
                runner_sources = {"angular/bzl/vitest-coverage.mjs"}
                runner_config = test.get("options", {}).get("runnerConfig")
                if isinstance(runner_config, str):
                    config_source = Path("angular") / runner_config
                    runner_sources.add(config_source.as_posix())
                    runner_sources.add(
                        config_source.with_name("vitest-global-setup.ts").as_posix()
                    )
                projects[prefix] = {
                    "target_prefix": "//angular/" + project["root"] + ":",
                    "runner_sources": runner_sources,
                }
    angular_sources = {
        name for name in sources if any(name.startswith(prefix) for prefix in projects)
    }
    native_sources = sorted(
        name
        for name in set(sources) - angular_sources
        if sources[name]["language"] == "javascript"
    )
    manifest = evidence / "native-manifest.txt"
    manifest.write_text("".join(name + "\n" for name in native_sources))
    native_directory = Path(tempfile.mkdtemp(prefix="native-", dir=run_directory))
    environment = dict(os.environ)
    environment.update(
        COVERAGE_MANIFEST=str(manifest),
        JAVASCRIPT_BASELINE_DIRECTORY=str(native_directory),
        NODE_V8_COVERAGE="",
        JS_COVERAGE__RUNFILES=str(root),
        TEST_WORKSPACE=".",
        BAZEL_BINDIR=".",
    )
    environment.pop("COVERAGE_OUTPUT_FILE", None)
    environment.pop("COVERAGE_DIR", None)
    subprocess.run([args.collector], cwd=root, env=environment, check=True)
    baseline_path = native_directory / "_rules_js_report.lcov"
    baseline = records(baseline_path) if native_sources else {}
    if set(native_sources) != set(baseline):
        raise ValueError(
            f"Incomplete native JavaScript baseline: missing={sorted(set(native_sources) - set(baseline))}, unexpected={sorted(set(baseline) - set(native_sources))}"
        )
    supported_baseline = {
        name: lines
        for name, lines in baseline.items()
        if Path(name).suffix in {".js", ".mjs", ".cjs"}
    }
    supported_baseline_path = run_directory / "native-supported.lcov"
    supported_baseline_path.write_text(
        "".join(
            "\n".join(lines) + "\nend_of_record\n"
            for lines in supported_baseline.values()
        )
    )
    combined = records(args.report)
    observed = {}
    raw_sources = set()
    test_reports = []
    limits = []
    tests = set(inventory["javascript_tests"])
    executions = []
    events = [json.loads(line) for line in args.events.read_text().splitlines()]
    named_sets = {
        event["id"]["namedSet"]["id"]: event["namedSetOfFiles"]
        for event in events
        if "namedSetOfFiles" in event
    }

    def artifacts(identifier: str, active: set[str]) -> list[Path]:
        if identifier in active:
            raise ValueError("JavaScript artifact sets contain a cycle.")
        entry = named_sets[identifier]
        paths = [
            local_path(file)
            if "uri" in file
            else root.joinpath(*file.get("pathPrefix", []), file["name"])
            for file in entry.get("files", [])
        ]
        for child in entry.get("fileSets", []):
            paths.extend(artifacts(child["id"], active | {identifier}))
        return paths

    configured = {}
    for event in events:
        for group in event.get("completed", {}).get("outputGroup", []):
            if group["name"] == "javascript_test_inputs":
                for entry in group.get("fileSets", []):
                    for path in artifacts(entry["id"], set()):
                        if not path.name.endswith(".javascript-test-inputs.json"):
                            continue
                        data = json.loads(path.read_text())
                        if not isinstance(data.get("sources"), list) or not isinstance(
                            data.get("instrumented"), bool
                        ):
                            raise TypeError(
                                f"Invalid configured JavaScript inputs: {path}"
                            )
                        configuration = (
                            event.get("id", {})
                            .get("targetCompleted", {})
                            .get("configuration", {})
                            .get("id")
                        )
                        configured[(data["rule"], configuration)] = data
                        saved = run_directory / (
                            str(len(configured)) + "-configured.json"
                        )
                        shutil.copyfile(path, saved)
                        inputs = run_directory / (str(len(configured)) + "-inputs")
                        inputs.mkdir(exist_ok=True)
                        for artifact in data.get("artifacts", []):
                            artifact_path = root / artifact["path"]
                            if artifact_path.is_dir():
                                continue
                            destination = inputs / artifact["path"]
                            destination.parent.mkdir(parents=True, exist_ok=True)
                            shutil.copyfile(artifact_path, destination)
                            if artifact_path.suffix == ".map":
                                source_map = json.loads(artifact_path.read_text())
                                if (
                                    source_map.get("version") != 3
                                    or not isinstance(source_map.get("sources"), list)
                                    or not isinstance(source_map.get("mappings"), str)
                                ):
                                    raise ValueError(
                                        f"Invalid required JavaScript source map: {artifact_path}"
                                    )
    for event in events:
        identity = event.get("id", {}).get("testResult", {})
        label = identity.get("label")
        if label not in tests:
            continue
        result = event["testResult"]
        configured_key = (label, identity.get("configuration", {}).get("id"))
        if (
            configured_key not in configured
            or not configured[configured_key]["instrumented"]
        ):
            raise ValueError(
                f"Missing required configured JavaScript instrumentation: {label}"
            )
        outputs = {item["name"]: item for item in result.get("testActionOutput", [])}
        if configured[configured_key].get("browser"):
            reports, browser_observed = browser_reports(
                outputs,
                run_directory / f"browser-{len(executions)}",
                set(sources),
                root,
                label,
            )
            test_reports.extend(reports)
            observed.update(browser_observed)
            for source in sorted(browser_observed):
                limits.append(
                    {
                        "source": source,
                        "rule": label,
                        "missing_guarantee": "Chromium coverage resets across page navigation. The retained final page profile does not establish complete measurement of earlier navigations.",
                        "evidence": [
                            str(run_directory / f"browser-{len(executions)}"),
                            "https://playwright.dev/docs/api/class-coverage#coverage-start-js-coverage",
                        ],
                    }
                )
        if "test.lcov" not in outputs:
            raise ValueError(f"Missing required JavaScript test report: {label}")
        report = local_path(outputs["test.lcov"])
        measured = records(report)
        for name, lines in measured.items():
            if (
                name in sources
                and any(line.startswith(("DA:", "BRDA:")) for line in lines)
                and name not in combined
            ):
                raise ValueError(
                    f"Missing required combined JavaScript source: {label}: {name}"
                )
        angular_test = any(
            label.startswith(project["target_prefix"]) for project in projects.values()
        )
        for name in set(configured[configured_key]["sources"]) & set(native_sources):
            if name not in measured:
                if angular_test and any(
                    label.startswith(project["target_prefix"])
                    and name in project["runner_sources"]
                    for project in projects.values()
                ):
                    limits.append(
                        {
                            "source": name,
                            "rule": label,
                            "record_absence": True,
                            "missing_guarantee": "The authoritative Vitest report omits runner configuration and global setup code outside its test instrumentation context.",
                            "evidence": str(report),
                        }
                    )
                else:
                    raise ValueError(
                        f"Missing required native JavaScript source: {label}: {name}"
                    )
        profiles = sorted((report.parent / "_coverage").glob("coverage-*.json"))
        saved_test = run_directory / f"test-{len(executions)}"
        saved_test.mkdir()
        shutil.copyfile(report, saved_test / "report.lcov")
        for profile in profiles:
            shutil.copyfile(profile, saved_test / profile.name)
        if not profiles:
            raise ValueError(f"Missing required native Node profile: {label}")
        for profile in profiles:
            data = json.loads(profile.read_text())
            if (
                not isinstance(data, dict)
                or not isinstance(data.get("result"), list)
                or not data["result"]
            ):
                raise ValueError(f"Invalid native Node profile: {profile}")
            for script in data["result"]:
                if (
                    not isinstance(script, dict)
                    or not isinstance(script.get("url"), str)
                    or not isinstance(script.get("functions"), list)
                ):
                    raise TypeError(f"Invalid native Node script profile: {profile}")
                for function in script["functions"]:
                    if (
                        not isinstance(function, dict)
                        or not isinstance(function.get("functionName"), str)
                        or not isinstance(function.get("ranges"), list)
                        or not function["ranges"]
                    ):
                        raise ValueError(
                            f"Invalid native Node function profile: {profile}"
                        )
                    for interval in function["ranges"]:
                        if (
                            not isinstance(interval, dict)
                            or any(
                                type(interval.get(key)) is not int
                                for key in ("startOffset", "endOffset", "count")
                            )
                            or not 0 <= interval["startOffset"] <= interval["endOffset"]
                            or interval["count"] < 0
                        ):
                            raise ValueError(
                                f"Invalid native Node range profile: {profile}"
                            )
        for name in sorted(set(configured[configured_key]["sources"]) & set(sources)):
            limits.append(
                {
                    "source": name,
                    "rule": label,
                    "missing_guarantee": "Native Node profiles do not declare the expected process set. A valid parent profile cannot prove that every executed child profile remains available.",
                    "evidence": [str(saved_test / path.name) for path in profiles],
                }
            )
        executions.append(
            {
                "rule": label,
                "report": str(saved_test / "report.lcov"),
                "original_report": str(report),
                "retained_profiles": [str(saved_test / path.name) for path in profiles],
                "profiles": [str(path) for path in profiles],
                "browser_outputs": str(run_directory / f"browser-{len(executions)}")
                if configured[configured_key].get("browser")
                else None,
                "cached_locally": result.get("cachedLocally", False),
                "execution": result.get("executionInfo", {}),
            }
        )
        supported = {name: lines for name, lines in measured.items() if name in sources}
        raw_sources.update(supported)
        if not angular_test:
            for name, lines in list(supported.items()):
                if (
                    Path(name).suffix in {".ts", ".mts", ".cts", ".tsx", ".jsx"}
                    and any(
                        line.startswith("FN:") and "(empty-report)" in line
                        for line in lines
                    )
                    and not has_hits(lines)
                ):
                    limits.append(
                        {
                            "source": name,
                            "rule": label,
                            "record_absence": True,
                            "missing_guarantee": "The native original-manifest path produced an empty-script fallback. It does not establish original executable records or measured zero coverage.",
                            "evidence": str(saved_test / "report.lcov"),
                        }
                    )
                    del supported[name]
        supported_report = saved_test / "supported.lcov"
        supported_report.write_text(
            "".join(
                "\n".join(lines) + "\nend_of_record\n" for lines in supported.values()
            )
        )
        test_reports.append(supported_report)
        observed.update({name: str(saved_test / "report.lcov") for name in supported})
        # The original per-test report establishes completeness before aggregate merging.
        for prefix in projects:
            package = projects[prefix]["target_prefix"]
            if not label.startswith(package):
                continue
            for name in sorted(angular_sources):
                if not name.startswith(prefix):
                    continue
                if re.search(r"\.(spec|test)\.[cm]?[jt]sx?$", name) or name.endswith(
                    ".d.ts"
                ):
                    limits.append(
                        {
                            "source": name,
                            "rule": label,
                            "record_absence": True,
                            "missing_guarantee": "The Angular builder excludes test sources and declarations from its coverage input.",
                            "evidence": str(report),
                        }
                    )
                elif name not in measured:
                    raise ValueError(
                        f"Missing required Angular source record: {label}: {name}"
                    )
    for name in sorted(angular_sources - set(observed)):
        if not any(item["source"] == name for item in limits):
            limits.append(
                {
                    "source": name,
                    "owners": sources[name]["owners"],
                    "record_absence": True,
                    "missing_guarantee": "The selected targets or execution filters did not run this Angular builder. Its documented coverage inclusion requires a test run.",
                    "evidence": str(args.events.resolve()),
                }
            )
    for name in sorted(set(native_sources) - set(supported_baseline) - set(observed)):
        limits.append(
            {
                "source": name,
                "owners": sources[name]["owners"],
                "record_absence": True,
                "missing_guarantee": "The native original-manifest baseline does not establish original TypeScript or JSX executable records. No supported mapped record is available in this run.",
                "evidence": str(baseline_path),
            }
        )
    for name, source in sorted(sources.items()):
        if source["language"] == "browser" and name not in observed:
            limits.append(
                {
                    "source": name,
                    "owners": source["owners"],
                    "record_absence": True,
                    "missing_guarantee": "The selected browser reports supply no map with the exact original Astro source content. Static build-time template execution and disconnected originals have no measured browser baseline. No processed-script bridge or original mapping is reconstructed.",
                    "evidence": [
                        str(run_directory),
                        "https://docs.astro.build/en/reference/configuration-reference/#vite",
                        "https://playwright.dev/docs/api/class-coverage",
                    ],
                }
            )
    for name in sorted(sources):
        limits.append(
            {
                "source": name,
                "owners": sources[name]["owners"],
                "missing_guarantee": "The maintained provider supplies its own source attribution and lowered branch semantics. Original executable-line and branch identities are not reconstructed.",
                "evidence": observed.get(
                    name,
                    str(baseline_path)
                    if name in baseline
                    else str(args.events.resolve()),
                ),
            }
        )
    # Remove scope exclusions and non-executable Angular template records.
    retained = {}
    for name, lines in combined.items():
        if name in raw_sources:
            continue
        if (
            name in native_sources
            and Path(name).suffix in {".ts", ".mts", ".cts", ".tsx", ".jsx"}
            and not has_hits(lines)
        ):
            continue
        if Path(name).suffix in EXTENSIONS and (
            name in excluded or name not in sources
        ):
            continue
        if name.startswith("angular/") and name.endswith(".html"):
            continue
        retained[name] = lines
    filtered = evidence / "filtered.lcov"
    filtered.write_text(
        "".join("\n".join(lines) + "\nend_of_record\n" for lines in retained.values())
    )
    # Establish required lines and hits from collector inputs before merging.
    expected: dict[str, dict[int, int]] = {}
    inputs = [filtered, supported_baseline_path, *test_reports]
    for path in inputs:
        if not path.read_text().strip():
            continue
        for name, lines in records(path).items():
            if name not in sources:
                continue
            counts = expected.setdefault(name, {})
            for line in lines:
                if line.startswith("DA:"):
                    number, count = (int(value) for value in line[3:].split(",")[:2])
                    counts[number] = max(counts.get(number, 0), count)
    reports = evidence / "reports.txt"
    reports.write_text(
        "".join(
            str(path) + "\n"
            for path in [filtered, supported_baseline_path, *test_reports]
        )
    )
    merged = evidence / "merged.lcov"
    subprocess.run(
        [args.merger, "--reports_file=" + str(reports), "--output_file=" + str(merged)],
        cwd=root,
        check=True,
    )
    final = records(merged)
    for name in set(supported_baseline) | set(observed):
        if name not in final:
            raise ValueError(f"Missing merged JavaScript source: {name}")
    for name, required in expected.items():
        actual = {
            int(line[3:].split(",")[0]): int(line[3:].split(",")[1])
            for line in final.get(name, [])
            if line.startswith("DA:")
        }
        if name not in final or any(
            number not in actual or actual[number] < count
            for number, count in required.items()
        ):
            raise ValueError(f"Missing merged JavaScript lines or hits: {name}")
    receipt = {
        "schema_version": 1,
        "native_sources": sorted(supported_baseline),
        "sources": {name: sorted(lines) for name, lines in sorted(expected.items())},
        "native_manifest_sources": native_sources,
        "observed_sources": observed,
        "executions": executions,
        "limits": limits,
        "baseline": str(supported_baseline_path),
        "raw_native_baseline": str(baseline_path),
        "events": str(run_directory / "events.json"),
        "inventory": str(run_directory / "inventory.json"),
        "report": str(args.report.resolve()),
    }
    (evidence / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    args.report.write_text(merged.read_text())


if __name__ == "__main__":
    main()
