#!/usr/bin/env python3
import json
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class ProductSurfaceGateTests(unittest.TestCase):
    def test_gate_is_deterministic_and_passes_current_surface(self):
        command = [sys.executable, str(ROOT / "scripts/check_product_surface.py"), "--json"]
        first = subprocess.run(command, cwd=ROOT, check=False, capture_output=True, text=True)
        second = subprocess.run(command, cwd=ROOT, check=False, capture_output=True, text=True)
        self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
        self.assertEqual(first.stdout, second.stdout)
        result = json.loads(first.stdout)
        self.assertEqual(result["status"], "PASS")
        self.assertTrue(all(check["status"] == "PASS" for check in result["checks"]))


if __name__ == "__main__":
    unittest.main()
