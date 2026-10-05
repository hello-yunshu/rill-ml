import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/generate_sbom.py"
sys.path.insert(0, str(ROOT / "scripts"))
import generate_sbom  # noqa: E402


class SbomTests(unittest.TestCase):
    def test_cargo_inventory_decodes_structured_output_as_utf8_bytes(self):
        package_id = "registry+https://example.invalid#index:rill-ml@1.3.0"
        metadata = {
            "packages": [
                {
                    "id": package_id,
                    "name": "rill-ml",
                    "version": "1.3.0",
                    "description": "中文说明 “curly quotes” café",
                    "source": None,
                    "checksum": None,
                }
            ],
            "resolve": {"nodes": []},
        }
        output = json.dumps(metadata, ensure_ascii=False).encode("utf-8")
        with patch(
            "generate_sbom.subprocess.run",
            return_value=SimpleNamespace(returncode=0, stdout=output, stderr=b""),
        ) as run:
            inventory, dependencies = generate_sbom.cargo_inventory()
        self.assertEqual(inventory[0]["name"], "rill-ml")
        self.assertEqual(dependencies, {package_id: []})
        self.assertNotIn("text", run.call_args.kwargs)

    def test_cargo_inventory_reports_process_encoding_and_json_failures(self):
        failures = [
            (1, b"", "cargo metadata failed with exit code 1"),
            (0, b"\xff", "invalid UTF-8"),
            (0, b"{broken", "invalid JSON"),
        ]
        for returncode, stdout, expected in failures:
            with self.subTest(expected=expected), patch(
                "generate_sbom.subprocess.run",
                return_value=SimpleNamespace(
                    returncode=returncode,
                    stdout=stdout,
                    stderr="cargo says 中文”.".encode("utf-8"),
                ),
            ):
                with self.assertRaisesRegex(generate_sbom.CargoMetadataError, expected):
                    generate_sbom.cargo_inventory()

    def test_failed_metadata_does_not_create_partial_sbom_output(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "sbom"
            arguments = [
                "generate_sbom.py",
                "--version", "1.3.0",
                "--tag", "v1.3.0",
                "--commit", "a" * 40,
                "--output-dir", str(output),
            ]
            with patch("generate_sbom.sys.argv", arguments), patch(
                "generate_sbom.cargo_inventory",
                side_effect=generate_sbom.CargoMetadataError("bad metadata"),
            ):
                self.assertEqual(generate_sbom.main(), 1)
            self.assertFalse(output.exists())

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
                f"rill-runtime-linux-x86_64_musl={artifact}",
            ]
            first = subprocess.run(command, cwd=ROOT, check=False, capture_output=True, text=True)
            self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
            first_bytes = {path.name: path.read_bytes() for path in output.iterdir()}
            second = subprocess.run(command, cwd=ROOT, check=False, capture_output=True, text=True)
            self.assertEqual(second.returncode, 0, second.stdout + second.stderr)
            self.assertEqual(first_bytes, {path.name: path.read_bytes() for path in output.iterdir()})

            cdx = json.loads((output / "rill-ml-1.3.0.cdx.json").read_text(encoding="utf-8"))
            self.assertEqual(cdx["metadata"]["properties"][0]["value"], "v1.3.0")
            artifact_component = next(component for component in cdx["components"] if component["name"] == "rill-runtime-linux-x86_64_musl")
            self.assertEqual(artifact_component["hashes"][0]["content"], hashlib.sha256(artifact.read_bytes()).hexdigest())
            spdx = json.loads((output / "rill-ml-1.3.0.spdx.json").read_text(encoding="utf-8"))
            self.assertEqual(spdx["name"], "rill-ml-1.3.0")
            self.assertEqual(spdx["files"][0]["fileName"], "rill-runtime-linux-x86_64_musl")
            self.assertNotIn("_", spdx["files"][0]["SPDXID"])

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
