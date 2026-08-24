import json
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class ActionPinsTest(unittest.TestCase):
    def test_workflow_actions_do_not_use_fixed_shas(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/check_action_pins.py", "--json"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["status"], "PASS")
        self.assertGreater(payload["referenceCount"], 0)


if __name__ == "__main__":
    unittest.main()
