"""Exercise the real bootstrap wrapper without undeclared Linux host tools."""

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


@unittest.skipUnless(sys.platform.startswith("linux"), "This test requires Linux.")
class BootstrapTest(unittest.TestCase):
    def test_symlinked_compiler_needs_no_host_path_tools(self) -> None:
        bash = shutil.which("bash")
        assert bash is not None, "The bootstrap runtime requires Bash."
        wrapper = (
            Path(os.environ["TEST_SRCDIR"])
            / os.environ["TEST_WORKSPACE"]
            / os.environ["BOOTSTRAP_WRAPPER"]
        ).resolve(strict=True)
        for relative in (False, True):
            with (
                self.subTest(relative_sdk_link=relative),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                sdk = root / "sdk"
                sdk.mkdir()
                compiler = sdk / "go"
                compiler.write_text(
                    f"#!{bash}\n"
                    'case "$1" in\n'
                    "  clean) exit 0 ;;\n"
                    "  build) shift ;;\n"
                    "  *) exit 4 ;;\n"
                    "esac\n"
                    "while (( $# )); do\n"
                    '  if [[ "$1" == -o ]]; then\n'
                    '    printf "bootstrap fixture\\n" > "$2"\n'
                    "    exit 0\n"
                    "  fi\n"
                    "  shift\n"
                    "done\n"
                    "exit 3\n"
                )
                compiler.chmod(0o700)
                link = root / "go"
                link.symlink_to(Path("sdk/go") if relative else compiler)
                empty_path = root / "empty-path"
                empty_path.mkdir()
                outputs = [root / name for name in ("pack", "covdata", "builder")]
                result = subprocess.run(
                    [
                        bash,
                        str(wrapper),
                        *(str(path) for path in outputs),
                        "builder.go",
                    ],
                    cwd=root,
                    env={
                        "PATH": str(empty_path),
                        "GO_BINARY": str(link),
                        "LD_FLAGS": "",
                    },
                    capture_output=True,
                    text=True,
                    timeout=10,
                    check=False,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                for output in outputs:
                    self.assertEqual(output.read_text(), "bootstrap fixture\n")


if __name__ == "__main__":
    unittest.main()
