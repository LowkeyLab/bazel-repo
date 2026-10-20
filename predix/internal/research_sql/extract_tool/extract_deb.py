"""Extract checksum-pinned PostgreSQL profiler files for the research container."""

import hashlib
import io
import json
import sys
import tarfile
from pathlib import Path


def main() -> None:
    source, output, manifest = map(Path, sys.argv[1:])
    package = source.read_bytes()
    if package[:8] != b"!<arch>\n":
        raise ValueError("The input is not a Debian archive")
    offset = 8
    payload = None
    while offset < len(package):
        header = package[offset : offset + 60]
        name = header[:16].decode().strip().rstrip("/")
        size = int(header[48:58])
        offset += 60
        if name.startswith("data.tar"):
            payload = package[offset : offset + size]
        offset += size + size % 2
    if payload is None:
        raise ValueError("The Debian archive has no data member")
    records = []
    with (
        tarfile.open(fileobj=io.BytesIO(payload), mode="r:*") as archive,
        tarfile.open(output, "w") as selected,
    ):
        for member in archive:
            name = member.name.removeprefix("./")
            allowed = (
                name == "usr/lib/postgresql/18/lib/plpgsql_check.so"
                or name.startswith("usr/share/postgresql/18/extension/plpgsql_check")
            )
            if not member.isfile() or not allowed:
                continue
            stream = archive.extractfile(member)
            if stream is None:
                raise ValueError("A profiler archive member has no data")
            data = stream.read()
            records.append({"path": name, "sha256": hashlib.sha256(data).hexdigest()})
            item = tarfile.TarInfo(name)
            item.size = len(data)
            item.mode = 0o644
            selected.addfile(item, io.BytesIO(data))
    if not any(row["path"].endswith("plpgsql_check.so") for row in records):
        raise ValueError("The profiler library is missing")
    manifest.write_text(
        json.dumps(
            {"package_sha256": hashlib.sha256(package).hexdigest(), "files": records},
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
