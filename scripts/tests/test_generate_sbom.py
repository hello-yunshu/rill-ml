import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/generate_sbom.py"


class SbomTests(unittest.TestCase):
    def test_cyclonedx_and_spdx_are_deterministic_and_bound_to_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            temp = Path(directory)
            artifact = temp / "runtime"
            artifact.write_bytes(b"deterministic artifact")
            output = temp / "out"
            command = [
                sys.executable,
                str(SCRIPT),
                "--version",
                "1.3.0",
                "--tag",
                "v1.3.0",
                "--commit",
                "a" * 40,
                "--output-dir",
                str(output),
                "--artifact",
                f"rill-runtime-linux-x86_64={artifact}",
            ]
            first = subprocess.run(command, cwd=ROOT, check=False, capture_output=True, text=True)
            self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
            first_bytes = {path.name: path.read_bytes() for path in output.iterdir()}
            second = subprocess.run(command, cwd=ROOT, check=False, capture_output=True, text=True)
            self.assertEqual(second.returncode, 0, second.stdout + second.stderr)
            self.assertEqual(first_bytes, {path.name: path.read_bytes() for path in output.iterdir()})

            cdx = json.loads((output / "rill-ml-1.3.0.cdx.json").read_text(encoding="utf-8"))
            self.assertEqual(cdx["metadata"]["properties"][0]["value"], "v1.3.0")
            artifact_component = next(component for component in cdx["components"] if component["name"] == "rill-runtime-linux-x86_64")
            self.assertEqual(artifact_component["hashes"][0]["content"], hashlib.sha256(artifact.read_bytes()).hexdigest())
            spdx = json.loads((output / "rill-ml-1.3.0.spdx.json").read_text(encoding="utf-8"))
            self.assertEqual(spdx["name"], "rill-ml-1.3.0")
            self.assertEqual(spdx["files"][0]["fileName"], "rill-runtime-linux-x86_64")

            verify = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "scripts/verify_sbom.py"),
                    "--cdx",
                    str(output / "rill-ml-1.3.0.cdx.json"),
                    "--spdx",
                    str(output / "rill-ml-1.3.0.spdx.json"),
                    "--version",
                    "1.3.0",
                    "--tag",
                    "v1.3.0",
                    "--commit",
                    "a" * 40,
                ],
                cwd=ROOT,
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(verify.returncode, 0, verify.stdout + verify.stderr)


if __name__ == "__main__":
    unittest.main()
