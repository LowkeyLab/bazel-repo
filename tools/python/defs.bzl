"""Python coverage and test conventions for Gazelle-generated targets."""

load("@aspect_rules_py//py:defs.bzl", _py_binary = "py_binary", _py_library = "py_library", _py_test = "py_test")

def _coverage_tool():
    return select({
        "//tools/python:coverage_enabled": "//tools/python:coverage_baseline",
        "//conditions:default": None,
    })

def py_library(name, tags = [], **kwargs):
    """Declare a library with a production coverage baseline.

    Args:
        name: Target name.
        tags: Existing target tags.
        **kwargs: Remaining rules_py library attributes.
    """
    _py_library(name = name, tags = tags + ["python-coverage"], coverage_baseline = _coverage_tool(), **kwargs)

def py_binary(name, tags = [], **kwargs):
    """Declare a binary with a production coverage baseline.

    Args:
        name: Target name.
        tags: Existing target tags.
        **kwargs: Remaining rules_py binary attributes.
    """
    _py_binary(name = name, tags = tags + ["python-coverage"], coverage_baseline = _coverage_tool(), **kwargs)

def py_test(name, main = None, deps = [], **kwargs):
    """Use pytest for generated test sources instead of a source entry point.

    Args:
        name: Target name.
        main: Gazelle's generated entry point, replaced by pytest.
        deps: Dependencies inferred by Gazelle.
        **kwargs: Remaining rules_py test attributes.
    """
    deps = list(deps)
    if "@pypi//pytest" not in deps:
        deps.append("@pypi//pytest")
    if "@pypi//coverage" not in deps:
        deps.append("@pypi//coverage")
    if "//tools/python:coverage_runtime" not in deps:
        deps.append("//tools/python:coverage_runtime")
    _py_test(
        name = name,
        pytest_main = True,
        coverage_baseline = _coverage_tool(),
        deps = deps,
        **kwargs
    )
