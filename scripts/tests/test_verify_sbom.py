import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class VerifySbomTests(unittest.TestCase):
    def test_identity_mismatch_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            temp = Path(directory)
            cdx = temp / "cdx.json"
            spdx = temp / "spdx.json"
            cdx.write_text(json.dumps({"bomFormat": "CycloneDX"}), encoding="utf-8")
            spdx.write_text(json.dumps({"spdxVersion": "SPDX-2.3"}), encoding="utf-8")
            result = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "scripts/verify_sbom.py"),
                    "--cdx",
                    str(cdx),
                    "--spdx",
                    str(spdx),
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
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("FAIL", result.stdout)


if __name__ == "__main__":
    unittest.main()
