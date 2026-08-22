import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
VALID = ROOT / "schemas/examples/rill-consumer-adoption-example.json"
SCRIPT = ROOT / "scripts/validate_consumer_adoption.py"


class ConsumerAdoptionTests(unittest.TestCase):
    def run_validator(self, value):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "record.json"
            path.write_text(json.dumps(value), encoding="utf-8")
            return subprocess.run(
                [sys.executable, str(SCRIPT), str(path), "--json"],
                cwd=ROOT,
                check=False,
                capture_output=True,
                text=True,
            )

    def test_example_is_valid(self):
        result = subprocess.run(
            [sys.executable, str(SCRIPT), str(VALID), "--json"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(json.loads(result.stdout)["status"], "PASS")

    def test_unknown_schema_version_fails_closed(self):
        value = json.loads(VALID.read_text(encoding="utf-8"))
        value["schemaVersion"] = 2
        result = self.run_validator(value)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(json.loads(result.stdout)["status"], "FAIL")

    def test_unknown_field_fails_closed(self):
        value = json.loads(VALID.read_text(encoding="utf-8"))
        value["authoritative"] = True
        result = self.run_validator(value)
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(any(error.startswith("unknown fields:") for error in json.loads(result.stdout)["errors"]))


if __name__ == "__main__":
    unittest.main()
