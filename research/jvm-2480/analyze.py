"""Inspect compiler metadata without loading application classes."""

import argparse
import hashlib
import json
import struct
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import cast
from zipfile import ZipFile


def class_metadata(data: bytes) -> dict[str, str | list[str]]:
    offset = 8
    count = int.from_bytes(data[offset : offset + 2], "big")
    offset += 2
    pool: list[str | int | None] = [None] * count
    index = 1
    while index < count:
        tag = data[offset]
        offset += 1
        if tag == 1:
            size = struct.unpack_from(">H", data, offset)[0]
            offset += 2
            pool[index] = data[offset : offset + size].decode("utf-8", errors="replace")
            offset += size
        elif tag in (7, 8, 16, 19, 20):
            pool[index] = struct.unpack_from(">H", data, offset)[0]
            offset += 2
        elif tag in (3, 4, 9, 10, 11, 12, 17, 18):
            offset += 4
        elif tag in (5, 6):
            offset += 8
            index += 1
        elif tag == 15:
            offset += 3
        else:
            raise ValueError(f"Unknown constant tag {tag}")
        index += 1
    _, this_class, _ = struct.unpack_from(">HHH", data, offset)
    offset += 6
    interfaces = struct.unpack_from(">H", data, offset)[0]
    offset += 2 + 2 * interfaces

    def attributes() -> dict[str, bytes]:
        nonlocal offset
        count = struct.unpack_from(">H", data, offset)[0]
        offset += 2
        result = {}
        for _ in range(count):
            name, size = struct.unpack_from(">HI", data, offset)
            offset += 6
            result[cast(str, pool[name])] = data[offset : offset + size]
            offset += size
        return result

    for _ in range(2):
        count = struct.unpack_from(">H", data, offset)[0]
        offset += 2
        for _ in range(count):
            offset += 6
            attributes()
    attrs = attributes()
    source = cast(str, pool[struct.unpack(">H", attrs["SourceFile"])[0]])
    return {
        "class": cast(str, pool[cast(int, pool[this_class])]),
        "source_file": source,
        "smap": attrs.get("SourceDebugExtension", b"").decode(),
        "sha256": hashlib.sha256(data).hexdigest(),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("evidence", type=Path)
    args = parser.parse_args()
    root = args.evidence
    owners = {}
    for rule in ET.parse(root / "source-owners.xml").getroot().findall("rule"):
        srcs = []
        for label in rule.findall("list[@name='srcs']/label"):
            package, name = label.attrib["value"][2:].split(":", 1)
            srcs.append(package + "/" + name)
        owners[rule.attrib["name"]] = srcs
    rows = []
    for line in (root / "java-info.txt").read_text().splitlines():
        target = json.loads(line)
        owner = target["owner"].replace("@@//", "//")
        if owner.endswith(":baseline"):
            continue
        for jar_path in target["jars"]:
            jar = root / "inputs" / Path(jar_path).name
            with ZipFile(jar) as archive:
                for entry in archive.namelist():
                    if not entry.endswith(".class.uninstrumented"):
                        continue
                    row = class_metadata(archive.read(entry))
                    candidates = [
                        s for s in owners[owner] if Path(s).name == row["source_file"]
                    ]
                    row.update(
                        owner=owner,
                        jar=jar.name,
                        candidates=candidates,
                        status="unique" if len(candidates) == 1 else "ambiguous",
                    )
                    rows.append(row)
    (root / "automatic-ownership.json").write_text(json.dumps(rows, indent=2) + "\n")
    print(
        json.dumps(
            {
                "classes": len(rows),
                "unique": sum(r["status"] == "unique" for r in rows),
                "ambiguous": [r["class"] for r in rows if r["status"] == "ambiguous"],
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
