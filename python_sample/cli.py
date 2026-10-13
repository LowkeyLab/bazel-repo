"""Command-line interface for the Python reference project."""

from argparse import ArgumentParser
from collections.abc import Sequence

from versions import sort_versions


def main(argv: Sequence[str] | None = None) -> None:
    """Print supplied versions in package release order."""
    parser = ArgumentParser(description="Sort Python package versions.")
    parser.add_argument("versions", nargs="+")
    args = parser.parse_args(argv)
    print("\n".join(sort_versions(args.versions)))
