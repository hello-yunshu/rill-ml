import json
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class ConformanceTests(unittest.TestCase):
    def run_mode(self, mode: str) -> tuple[int, dict[str, object]]:
        result = subprocess.run(
            [sys.executable, "conformance/run.py", "--mode", mode, "--json"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        return result.returncode, json.loads(result.stdout)

    def test_offline_is_pass(self) -> None:
        code, report = self.run_mode("offline")
        self.assertEqual(code, 0)
        self.assertEqual(report["status"], "PASS")

    def test_released_without_inputs_is_blocked(self) -> None:
        code, report = self.run_mode("released")
        self.assertEqual(code, 2)
        self.assertEqual(report["status"], "BLOCKED")

    def test_explicit_skip_is_not_run(self) -> None:
        code, report = self.run_mode("not-run")
        self.assertEqual(code, 0)
        self.assertEqual(report["status"], "NOT_RUN")


if __name__ == "__main__":
    unittest.main()
