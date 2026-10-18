"""Collect child execution in the private Bazel test environment."""

import atexit
import json
import os
import sys
import sysconfig
import traceback
import uuid
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from coverage import Coverage


HOOK = 'import os, runpy; os.environ.get("BAZEL_PYTHON_COVERAGE") and runpy.run_path(os.environ["BAZEL_PYTHON_COVERAGE"], run_name="__bazel_coverage_child__")\n'


def start(manifest: list[str]) -> Coverage:
    """Start parent collection and prepare isolated child collection."""
    import coverage
    from coverage.files import abs_file

    root = Path(os.environ["COVERAGE_DIR"]).absolute() / "python-processes"
    root.mkdir()
    collector = coverage.Coverage(
        include=[abs_file(source) for source in manifest],
        branch=True,
        data_file=str(root / ".coverage"),
        data_suffix=True,
    )
    # Read the caller's configuration once. Children must use the same options.
    collector.set_option("run:parallel", True)
    rcfile = os.environ.get("COVERAGE_RCFILE")
    config = root / "config.json"
    config.write_text(
        json.dumps(
            {
                "coverage_path": str(Path(coverage.__file__).parent.parent),
                "data_file": str(root / ".coverage"),
                "include": collector.get_option("run:include"),
                "rcfile": str(Path(rcfile).absolute()) if rcfile else True,
            }
        )
    )
    hook = root / "startup.pth"
    hook.write_text(HOOK)
    os.environ["BAZEL_PYTHON_COVERAGE"] = str(Path(__file__).absolute())
    os.environ["BAZEL_PYTHON_COVERAGE_CONFIG"] = str(config)
    os.environ["BAZEL_PYTHON_COVERAGE_HOOK"] = str(hook)
    # sys.executable children reuse this test's private virtual environment.
    (Path(sysconfig.get_path("purelib")) / "bazel_coverage.pth").write_text(HOOK)
    collector.start()
    return collector


def finish(collector: Coverage) -> None:
    """Require complete child data before combining the process results."""
    collector.stop()
    _combine(collector)


def _combine(collector: Coverage) -> None:
    import coverage

    collector.save()
    root = Path(os.environ["BAZEL_PYTHON_COVERAGE_CONFIG"]).parent
    for pending in root.glob("*.started"):
        completed = pending.with_suffix(".saved")
        if not completed.is_file():
            raise RuntimeError(f"Python child coverage did not finish: {pending.name}")
        data_file = Path(completed.read_text())
        if not data_file.is_file():
            raise RuntimeError(f"Python child coverage data is missing: {data_file}")
    # Read and merge each file directly so corrupt inputs cannot become warnings.
    for data_file in root.glob(".coverage.*"):
        if str(data_file) == collector.get_data().data_filename():
            continue
        data = coverage.CoverageData(basename=str(data_file))
        data.read()
        collector.get_data().update(data)


def _save_child(collector: Coverage, receipt: Path) -> None:
    collector.save()
    receipt.with_suffix(".saved").write_text(collector.get_data().data_filename())


def child() -> None:
    """Start collection or terminate the child when collection fails."""
    try:
        config_path = Path(os.environ["BAZEL_PYTHON_COVERAGE_CONFIG"])
        receipt = config_path.parent / f"{os.getpid()}-{uuid.uuid4()}.started"
        receipt.touch()
        config = json.loads(config_path.read_text())
        sys.path.insert(0, config["coverage_path"])
        import coverage

        collector = coverage.Coverage(
            config_file=config["rcfile"],
            include=config["include"],
            branch=True,
            data_file=config["data_file"],
            data_suffix=True,
        )
        collector.set_option("run:parallel", True)
        collector.start()

        def save() -> None:
            try:
                collector.stop()
                _save_child(collector, receipt)
            except Exception:  # noqa: BLE001 -- Startup and exit errors must terminate collection.
                traceback.print_exc()
                os._exit(1)

        atexit.register(save)
    except Exception:  # noqa: BLE001 -- Startup and exit errors must terminate collection.
        # Python normally prints .pth errors and continues without collection.
        traceback.print_exc()
        os._exit(1)


if __name__ == "__bazel_coverage_child__":
    child()
