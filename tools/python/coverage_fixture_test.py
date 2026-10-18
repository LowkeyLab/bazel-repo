"""Exercise real process and collector failures through the test runner."""

import os
import subprocess
import sys
from pathlib import Path


def test_child_finishes() -> None:
    environment = dict(os.environ)
    if environment.get("COVERAGE_TEST_FAULT") == "collection":
        environment["COVERAGE_CORE"] = "invalid"
    subprocess.run(
        [sys.executable, "-c", "print('child completed')"],
        env=environment,
        check=False,
        capture_output=True,
    )
    if environment.get("COVERAGE_TEST_FAULT") == "combination":
        # Corrupt a real collector output after the child finishes.
        for data_file in Path(os.environ["COVERAGE_DIR"]).rglob(".coverage.*"):
            data_file.write_bytes(b"invalid coverage data")
