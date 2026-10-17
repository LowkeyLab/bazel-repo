"""Exercise package ordering through the generated dependency graph."""

import pytest
from packaging.version import InvalidVersion

from versions import sort_versions


@pytest.mark.parametrize(
    ("versions", "expected"),
    [
        (["10.0", "2.0", "1.0"], ["1.0", "2.0", "10.0"]),
        (
            ["1.0.post1", "1.0", "1.0rc1", "1.0.dev1"],
            ["1.0.dev1", "1.0rc1", "1.0", "1.0.post1"],
        ),
        ([], []),
    ],
)
def test_package_release_order(versions: list[str], expected: list[str]) -> None:
    assert sort_versions(versions) == expected


def test_invalid_version_is_rejected() -> None:
    with pytest.raises(InvalidVersion):
        sort_versions(["not-a-version"])
