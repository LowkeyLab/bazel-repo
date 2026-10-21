"""Check native Go coverage preservation across supported SDK line models."""

import pytest

from tools.python.coverage_membership_test import assert_go_coverage

# These executable sets come from fresh native Go 1.26.4 and 1.27.3 reports.
GO_126 = """SF:predix/internal/domain/user/user.go
FNF:0
FNH:0
DA:28,1
DA:29,1
DA:30,1
DA:33,1
DA:34,1
DA:35,0
DA:36,0
DA:38,1
DA:39,1
DA:40,1
DA:41,1
DA:42,1
DA:43,1
LH:11
LF:13
""".splitlines()
GO_127 = [
    line for line in GO_126 if line not in {"DA:28,1", "DA:33,1", "LH:11", "LF:13"}
] + ["LH:9", "LF:11"]


@pytest.mark.parametrize("record", [GO_126, GO_127], ids=["go-1.26", "go-1.27"])
def test_native_line_models_preserve_measured_hits(record: list[str]) -> None:
    assert_go_coverage(record, record)


@pytest.mark.parametrize("record", [GO_126, GO_127], ids=["go-1.26", "go-1.27"])
def test_lost_body_hit_causes_failure(record: list[str]) -> None:
    actual = ["DA:40,0" if line == "DA:40,1" else line for line in record]
    actual = [
        f"LH:{int(line[3:]) - 1}" if line.startswith("LH:") else line for line in actual
    ]
    with pytest.raises(AssertionError):
        assert_go_coverage(actual, record)


@pytest.mark.parametrize("record", [GO_126, GO_127], ids=["go-1.26", "go-1.27"])
def test_native_report_requires_known_body_hits(record: list[str]) -> None:
    empty = [
        line.rsplit(",", 1)[0] + ",0" if line.startswith("DA:") else line
        for line in record
    ]
    empty = ["LH:0" if line.startswith("LH:") else line for line in empty]
    with pytest.raises(AssertionError):
        assert_go_coverage(empty, empty)
