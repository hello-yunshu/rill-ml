import json
import subprocess
import sys
import unittest
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from check_action_pins import scan


ROOT = Path(__file__).resolve().parents[2]


class ActionPinsTest(unittest.TestCase):
    def test_repository_workflow_actions_use_readable_refs(self) -> None:
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

    def test_fixed_sha_fails(self) -> None:
        result = self._scan("uses: actions/checkout@0123456789abcdef0123456789abcdef01234567\n")
        self.assertEqual(result["status"], "FAIL")

    def test_allowlisted_readable_refs_pass(self) -> None:
        for action, ref in (("actions/checkout", "v7"), ("dtolnay/rust-toolchain", "stable")):
            result = self._scan(f"uses: {action}@{ref}\n")
            self.assertEqual(result["status"], "PASS", f"{action}@{ref}")

    def test_forbidden_floating_refs_fail(self) -> None:
        for ref in ("latest", "HEAD", "main"):
            result = self._scan(f"uses: actions/checkout@{ref}\n")
            self.assertEqual(result["status"], "FAIL", ref)

    def test_unknown_external_action_fails(self) -> None:
        result = self._scan("uses: example/untrusted-action@v1\n")
        self.assertEqual(result["status"], "FAIL")

    def test_missing_ref_fails(self) -> None:
        result = self._scan("uses: actions/checkout\n")
        self.assertEqual(result["status"], "FAIL")

    def test_local_action_is_allowed(self) -> None:
        result = self._scan("uses: ./.github/actions/local\n")
        self.assertEqual(result["status"], "PASS")

    @staticmethod
    def _scan(workflow: str) -> dict[str, object]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workflow_dir = root / ".github" / "workflows"
            workflow_dir.mkdir(parents=True)
            scripts_dir = root / "scripts"
            scripts_dir.mkdir()
            (scripts_dir / "action-ref-policy.json").write_text(
                (ROOT / "scripts" / "action-ref-policy.json").read_text(encoding="utf-8"),
                encoding="utf-8",
            )
            (workflow_dir / "test.yml").write_text(workflow, encoding="utf-8")
            return scan(root)


if __name__ == "__main__":
    unittest.main()
