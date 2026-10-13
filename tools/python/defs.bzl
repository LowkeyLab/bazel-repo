"""Python test conventions for Gazelle-generated targets."""

load("@aspect_rules_py//py:defs.bzl", _py_test = "py_test")

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
    _py_test(
        name = name,
        pytest_main = True,
        deps = deps,
        **kwargs
    )
