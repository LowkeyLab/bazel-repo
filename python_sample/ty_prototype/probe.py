"""Disposable probes for future-target discovery, annotations, and imports."""

import math
from typing import Any, TypeForm

from local_helper import typed_helper
from packaging.version import Version
from ty_proto_stub import external_function

from versions import sort_versions


def compatible(value: str) -> list[str]:
    """Exercise typed third-party and transitive first-party imports."""
    return sort_versions([str(Version(value))])


def unannotated(value):
    """Probe whether missing annotations are rejected."""
    return value


def explicit_any(value: Any) -> Any:
    """Probe whether explicit Any is rejected."""
    return value


def type_form(value: TypeForm[int]) -> TypeForm[int]:
    """Exercise selected-version stdlib typing."""
    return value


prototype_future_value: int = 1
own_root_value: str = typed_helper("ok")
stub_value: int = external_function("ok")
math_value: float = math.sqrt(4)
