"""Merge standard JaCoCo analysis with exact configured source ownership."""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import xml.etree.ElementTree as ET
import zipfile
from collections import defaultdict
from pathlib import Path
from typing import NotRequired, TypedDict
from urllib.parse import unquote, urlparse


class FileReference(TypedDict):
    name: str
    uri: NotRequired[str]
    pathPrefix: NotRequired[list[str]]


def local_file(value: FileReference, root: Path) -> Path:
    uri = str(value.get("uri", ""))
    if uri:
        parsed = urlparse(uri)
        if parsed.scheme != "file":
            raise ValueError("Required JVM output is not available locally: " + uri)
        return Path(unquote(parsed.path))
    return root.joinpath(*value.get("pathPrefix", []), str(value["name"]))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--events", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--analyzer", type=Path, required=True)
    args = parser.parse_args()
    root = Path(os.environ.get("BUILD_WORKSPACE_DIRECTORY", os.getcwd()))
    evidence = root / args.evidence
    evidence.mkdir(exist_ok=True)
    (evidence / "receipt.json").unlink(missing_ok=True)
    try:
        inventory = json.loads((root / args.inventory).read_text())
        events = [
            json.loads(line) for line in (root / args.events).read_text().splitlines()
        ]
        sets = {
            event["id"]["namedSet"]["id"]: event["namedSetOfFiles"]
            for event in events
            if "namedSetOfFiles" in event
        }

        def files(identifier: str) -> list[Path]:
            entry = sets[identifier]
            return [local_file(value, root) for value in entry.get("files", [])] + [
                path
                for nested in entry.get("fileSets", [])
                for path in files(nested["id"])
            ]

        roots = {
            label
            for label in inventory["coverage_roots"]
            if label.endswith(".jvm_coverage")
        }
        declarations = {}
        for event in events:
            label = event.get("id", {}).get("targetCompleted", {}).get("label")
            if label not in roots:
                continue
            completed = event["completed"]
            if not completed.get("success"):
                raise ValueError(f"Required JVM root failed: {label}")
            groups = [
                group
                for group in completed.get("outputGroup", [])
                if group["name"] == "jvm_coverage_baselines"
            ]
            paths = [
                path
                for group in groups
                for reference in group.get("fileSets", [])
                for path in files(reference["id"])
            ]
            metadata = [path for path in paths if path.suffix == ".json"]
            baselines = [path for path in paths if path.suffix == ".xml"]
            if len(metadata) != 1 or len(baselines) != 1:
                raise ValueError(f"Missing required JVM baseline artifacts: {label}")
            data = json.loads(metadata[0].read_text())
            jars = [root / path for path in data["jars"]]
            if (data["sources"] and not jars) or any(
                not jar.is_file() or jar.stat().st_size == 0 for jar in jars
            ):
                raise ValueError(f"Missing required JVM class input: {label}")
            baseline = ET.parse(baselines[0]).getroot()
            if baseline.tag != "report":
                raise ValueError(f"Invalid required JaCoCo baseline: {label}")
            declarations[label] = (data, baselines[0], baseline, jars)
        if roots != declarations.keys():
            raise ValueError(
                f"Missing required JVM roots: {sorted(roots - declarations.keys())}"
            )
        eligible = {
            source["path"]
            for source in inventory["eligible"]
            if source["language"] == "jvm"
        }
        declared = {
            name for data, _, _, _ in declarations.values() for name in data["sources"]
        }
        limits = []
        root_indexes = {
            label: index for index, label in enumerate(sorted(declarations))
        }
        for source in inventory["eligible"]:
            name = source["path"]
            if source["language"] != "jvm" or name in declared:
                continue
            configured = [
                (label, data)
                for label, (data, _, _, _) in declarations.items()
                if name in inventory.get("jvm_source_declarations", {}).get(label, [])
            ]
            if not configured:
                raise ValueError(f"JVM source has no configured analysis root: {name}")
            for label, _ in configured:
                limits.append(
                    {
                        "source": name,
                        "rule": label,
                        "evidence": [
                            str(root / args.inventory),
                            str(evidence / f"inputs-{root_indexes[label]}.json"),
                        ],
                        "missing_guarantee": "The configured source selection excludes this inventoried source.",
                    }
                )
        class_owners = defaultdict(set)
        identities = defaultdict(set)
        fingerprints = {}
        uncertain_identities = set()
        identity_owners = defaultdict(set)
        for label, (data, _, baseline, jars) in declarations.items():
            for node in baseline.findall("./package/class"):
                class_owners[node.attrib["name"]].add(label)
            for jar in jars:
                with zipfile.ZipFile(jar) as archive:
                    for node in baseline.findall("./package/class"):
                        name = node.attrib["name"]
                        entry = name + ".class"
                        if entry + ".uninstrumented" in archive.namelist():
                            entry += ".uninstrumented"
                        if entry not in archive.namelist():
                            continue
                        digest = hashlib.sha256(archive.read(entry)).hexdigest()
                        fingerprints[label, name] = digest
                        candidates = [
                            path
                            for path in data["sources"]
                            if Path(path).name == node.get("sourcefilename")
                        ]
                        identity_owners[name, digest].add(label)
                        identities[name, digest].update(candidates)
                        if len(candidates) != 1:
                            uncertain_identities.add((name, digest))
        shared_names = {
            name for name, owners in class_owners.items() if len(owners) > 1
        }
        unmatched = defaultdict(set)
        ambiguous_sources = set()
        executed = {
            owner["label"]
            for source in inventory["eligible"]
            if source["language"] == "jvm"
            for owner in source["owners"]
            if owner["kind"] in ("java_test", "kt_jvm_test")
        }
        executed.update(inventory.get("jvm_tests", []))
        profiles = set()
        execution = []
        for event in events:
            label = event.get("id", {}).get("testResult", {}).get("label")
            if "testResult" not in event:
                continue
            result = event["testResult"]
            logs = [
                local_file(value, root)
                for value in result.get("testActionOutput", [])
                if value["name"] == "test.log"
            ]
            if len(logs) != 1:
                if label in executed:
                    raise ValueError(f"Missing JVM execution outputs: {label}")
                continue
            directory = logs[0].parent / "_coverage"
            found = list(directory.glob("*.exec"))
            nonempty = [profile for profile in found if profile.stat().st_size]
            if not nonempty:
                if label in executed:
                    raise ValueError(f"Missing or empty required JVM profiles: {label}")
                continue
            profiles.update(nonempty)
            execution.append(
                {
                    "rule": label,
                    "profiles": [str(path) for path in nonempty],
                    "empty_profiles": [
                        str(path) for path in found if not path.stat().st_size
                    ],
                    "cached": result.get("cachedLocally", False),
                    "execution": result.get("executionInfo", {}),
                }
            )
        observed: dict[str, dict[int, int]] = defaultdict(dict)
        branches: dict[str, dict[int, tuple[int, int]]] = defaultdict(dict)
        mappings = []
        empty_sources = []
        retained_profiles = []
        for index, profile in enumerate(sorted(profiles)):
            retained = evidence / f"profile-{index}.exec"
            shutil.copyfile(profile, retained)
            retained_profiles.append(
                {"original": str(profile), "retained": str(retained)}
            )
        for index, (label, (data, baseline_path, baseline, jars)) in enumerate(
            sorted(declarations.items())
        ):
            report = evidence / f"analysis-{index}.xml"
            for child in data.get("children", []):
                for source in inventory["eligible"]:
                    if source["language"] == "jvm" and any(
                        owner["label"] == child for owner in source["owners"]
                    ):
                        limits.append(
                            {
                                "source": source["path"],
                                "rule": child,
                                "collection_rule": label,
                                "evidence": str(evidence / f"inputs-{index}.json"),
                                "missing_guarantee": "JaCoCo profiles cannot distinguish an unexecuted child JVM from a child whose complete profile disappeared.",
                            }
                        )
            shutil.copyfile(baseline_path, evidence / f"baseline-{index}.xml")
            (evidence / f"inputs-{index}.json").write_text(
                json.dumps(data, indent=2) + "\n"
            )
            for jar_index, jar in enumerate(jars):
                shutil.copyfile(jar, evidence / f"class-{index}-{jar_index}.jar")
            command = [str((root / args.analyzer).resolve()), str(report)]
            for jar in jars:
                command.extend(["--classfiles", str(jar)])
            for profile in sorted(profiles):
                command.extend(["--exec", str(profile)])
            for name in sorted(shared_names):
                command.extend(["--allow-class-name", name])
            result = subprocess.run(
                command, cwd=root, text=True, capture_output=True, check=False
            )
            (evidence / f"analysis-{index}.log").write_text(
                result.stdout + result.stderr
            )
            result.check_returncode()
            for line in result.stdout.splitlines():
                if line.startswith("JACOCO_NO_MATCH "):
                    unmatched[line.removeprefix("JACOCO_NO_MATCH ")].add(label)
            analysis = ET.parse(report).getroot()
            if analysis.tag != "report":
                raise ValueError(f"Invalid required JaCoCo analysis: {label}")
            if {c.attrib["name"] for c in baseline.findall("./package/class")} != {
                c.attrib["name"] for c in analysis.findall("./package/class")
            }:
                raise ValueError(f"Incomplete JaCoCo class analysis: {label}")

            def source_lines(document: ET.Element) -> dict[tuple[str, str], set[int]]:
                return {
                    (package.attrib["name"], source.attrib["name"]): {
                        int(line.attrib["nr"]) for line in source.findall("line")
                    }
                    for package in document.findall("package")
                    for source in package.findall("sourcefile")
                }

            if source_lines(baseline) != source_lines(analysis):
                raise ValueError(f"Incomplete JaCoCo source analysis: {label}")
            for node in analysis.findall("./package/class"):
                name = node.attrib["name"]
                digest = fingerprints.get((label, name))
                candidates = identities.get((name, digest), set())
                ambiguous = len(candidates) > 1 or (
                    (name, digest) in uncertain_identities
                    and len(identity_owners[name, digest]) > 1
                )
                if ambiguous and any(
                    int(counter.get("covered", "0")) > 0
                    for counter in node.findall("counter")
                    if counter.get("type") == "INSTRUCTION"
                ):
                    ambiguous_sources.update(candidates)
                    for source in sorted(candidates):
                        limits.append(
                            {
                                "source": source,
                                "rule": label,
                                "class": name,
                                "sha256": digest,
                                "evidence": str(report),
                                "missing_guarantee": "A JVM class identity is shared by distinct original sources. JaCoCo cannot identify the executed owner.",
                            }
                        )
            by_name = defaultdict(list)
            for name in data["sources"]:
                by_name[Path(name).name].append(name)
            mapped = set()
            for package in analysis.findall("package"):
                for source in package.findall("sourcefile"):
                    candidates = by_name[source.attrib["name"]]
                    if len(candidates) != 1:
                        mapped.update(candidates)
                        for name in candidates:
                            limits.append(
                                {
                                    "source": name,
                                    "rule": label,
                                    "evidence": str(report),
                                    "missing_guarantee": "JaCoCo source identity is ambiguous within this configured owner.",
                                }
                            )
                        continue
                    name = candidates[0]
                    mapped.add(name)
                    if name not in eligible:
                        continue
                    observed.setdefault(name, {})
                    count = len((root / name).read_text().splitlines())
                    for line in source.findall("line"):
                        number, missed, covered, mb, cb = (
                            int(line.attrib[key])
                            for key in ("nr", "mi", "ci", "mb", "cb")
                        )
                        if number <= 0 or min(missed, covered, mb, cb) < 0:
                            raise ValueError(f"Invalid JaCoCo line: {name}")
                        if number > count:
                            limits.append(
                                {
                                    "source": name,
                                    "rule": label,
                                    "evidence": str(report),
                                    "missing_guarantee": f"JaCoCo reports compiler-mapped line {number} beyond the original source.",
                                }
                            )
                            continue
                        if missed + covered:
                            observed[name][number] = max(
                                observed[name].get(number, 0), int(covered > 0)
                            )
                        if mb + cb:
                            prior = branches[name].get(number)
                            if prior is not None and prior != (mb, cb):
                                raise ValueError(
                                    f"Conflicting JaCoCo branch attribution: {name}:{number}"
                                )
                            branches[name][number] = (mb, cb)
                    mappings.append(
                        {
                            "source": name,
                            "rule": label,
                            "classes": [
                                c.attrib["name"]
                                for c in package.findall("class")
                                if c.attrib.get("sourcefilename")
                                == source.attrib["name"]
                            ],
                            "baseline": str(baseline_path),
                            "analysis": str(report),
                        }
                    )
                    if name.endswith(".kt"):
                        limits.append(
                            {
                                "source": name,
                                "rule": label,
                                "evidence": str(report),
                                "missing_guarantee": "Standard JaCoCo Kotlin filters and compiler mappings do not establish original inline branch identities.",
                            }
                        )
            for name in sorted(set(data["sources"]) - mapped):
                if name in eligible:
                    if all(
                        node.get("sourcefilename") is not None
                        for node in baseline.findall("./package/class")
                    ):
                        observed.setdefault(name, {})
                        empty_sources.append(
                            {
                                "source": name,
                                "rule": label,
                                "evidence": str(baseline_path),
                                "diagnostic": "The configured compiler output contains no class for this source.",
                            }
                        )
                        continue
                    limits.append(
                        {
                            "source": name,
                            "rule": label,
                            "evidence": str(baseline_path),
                            "missing_guarantee": "The configured class analysis supplies no unique source record. This is not measured zero coverage.",
                        }
                    )
        for name, owners in unmatched.items():
            if owners == class_owners[name]:
                raise ValueError(
                    f"No configured class matches JaCoCo execution data: {name}"
                )
        for name in ambiguous_sources:
            observed.pop(name, None)
            branches.pop(name, None)
        original = (root / args.report).read_text()
        records = original.split("end_of_record")
        if len(records) < 2 or records[-1].strip():
            raise ValueError("The combined coverage report is empty or incomplete.")
        retained = []
        for record in records[:-1]:
            sources = [
                line[3:] for line in record.splitlines() if line.startswith("SF:")
            ]
            if len(sources) != 1:
                raise ValueError("A combined record must identify one source.")
            if not sources[0].endswith((".java", ".kt", ".kts", ".clj")):
                retained.append(record + "end_of_record\n")
        for name, lines in sorted(observed.items()):
            record = [f"SF:{name}"]
            record.extend(f"DA:{number},{hit}" for number, hit in sorted(lines.items()))
            record.extend([f"LF:{len(lines)}", f"LH:{sum(lines.values())}"])
            if branches[name]:
                limits.append(
                    {
                        "source": name,
                        "evidence": [
                            mapping["analysis"]
                            for mapping in mappings
                            if mapping["source"] == name
                        ],
                        "rules": sorted(
                            {
                                mapping["rule"]
                                for mapping in mappings
                                if mapping["source"] == name
                            }
                        ),
                        "missing_guarantee": "Standard JaCoCo reports branch totals for each line without individual branch identities or execution counts.",
                    }
                )
            record.append("end_of_record\n")
            retained.append("\n".join(record))
        (evidence / "receipt.json").write_text(
            json.dumps(
                {
                    "roots": sorted(roots),
                    "sources": {
                        name: sorted(lines) for name, lines in observed.items()
                    },
                    "limits": limits,
                    "mappings": mappings,
                    "execution": execution,
                    "profiles": retained_profiles,
                    "empty_sources": empty_sources,
                    "branch_totals": {
                        name: {
                            "missed": sum(missed for missed, _ in counters.values()),
                            "covered": sum(covered for _, covered in counters.values()),
                        }
                        for name, counters in branches.items()
                    },
                },
                indent=2,
            )
            + "\n"
        )
        (root / args.report).write_text("".join(retained))
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        ET.ParseError,
        zipfile.BadZipFile,
        subprocess.CalledProcessError,
    ) as error:
        parser.exit(1, f"JVM coverage report failed: {error}\n")


if __name__ == "__main__":
    main()
