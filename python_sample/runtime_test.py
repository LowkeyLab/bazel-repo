"""Check the selected interpreter and isolated execution in CI."""

import os
import re
import site
import sys
from pathlib import Path


def test_runtime_matches_configured_python_minor():
    module_file = (
        Path(os.environ["TEST_SRCDIR"]) / os.environ["TEST_WORKSPACE"] / "MODULE.bazel"
    )
    selected = re.search(
        r'# renovate: datasource=python-version depName=python\s+python_version = "(3\.\d+)"',
        module_file.read_text(),
    )
    assert selected is not None
    expected = tuple(int(part) for part in selected.group(1).split("."))
    assert sys.version_info[:2] == expected


def test_runtime_ignores_ambient_python_configuration():
    assert sys.flags.isolated == 1
    assert site.ENABLE_USER_SITE is False
