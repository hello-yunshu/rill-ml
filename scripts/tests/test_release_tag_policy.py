"""Unit tests for ``scripts/release_tag_policy.py``.

The auto-release workflow delegates the tag-immutability decision to this
helper so we can cover every branch without mocking GitHub. The six cases
below mirror the audit-prompt requirements in section 6.1.
"""

from __future__ import annotations

import importlib.util
import pathlib
import sys
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "release_tag_policy.py"


def _load_module():
    spec = importlib.util.spec_from_file_location("release_tag_policy", MODULE_PATH)
    module = importlib.util.module_from_spec(spec)
    sys.modules["release_tag_policy"] = module
    spec.loader.exec_module(module)
    return module


policy = _load_module()


class ReleaseTagPolicyTest(unittest.TestCase):
    def test_new_tag_dispatches(self) -> None:
        decision = policy.decide_release_tag(
            tag_exists=False,
            tag_sha=None,
            target_sha="abc123",
            has_successful_release=False,
            has_active_release=False,
        )
        self.assertEqual(decision.action, "dispatch")
        self.assertIn("abc123", decision.reason)

    def test_existing_tag_with_matching_sha_dispatches_retry(self) -> None:
        decision = policy.decide_release_tag(
            tag_exists=True,
            tag_sha="abc123",
            target_sha="abc123",
            has_successful_release=False,
            has_active_release=False,
        )
        self.assertEqual(decision.action, "dispatch")
        self.assertIn("retrying", decision.reason)

    def test_existing_tag_with_different_sha_fails(self) -> None:
        decision = policy.decide_release_tag(
            tag_exists=True,
            tag_sha="oldsha",
            target_sha="newsha",
            has_successful_release=False,
            has_active_release=False,
        )
        self.assertEqual(decision.action, "fail")
        self.assertIn("immutable", decision.reason)
        self.assertIn("oldsha", decision.reason)
        self.assertIn("newsha", decision.reason)

    def test_existing_tag_with_different_sha_fails_even_with_successful_release(self) -> None:
        # Regression: a stale tag with a successful Release run must NOT be
        # silently skipped. The immutability check has to win so the
        # maintainer is alerted to bump the version.
        decision = policy.decide_release_tag(
            tag_exists=True,
            tag_sha="oldsha",
            target_sha="newsha",
            has_successful_release=True,
            has_active_release=False,
        )
        self.assertEqual(decision.action, "fail")
        self.assertIn("immutable", decision.reason)
        self.assertIn("oldsha", decision.reason)
        self.assertIn("newsha", decision.reason)

    def test_existing_tag_with_different_sha_fails_even_with_active_release(self) -> None:
        # Same regression guard as above, but for an in-flight Release run.
        decision = policy.decide_release_tag(
            tag_exists=True,
            tag_sha="oldsha",
            target_sha="newsha",
            has_successful_release=False,
            has_active_release=True,
        )
        self.assertEqual(decision.action, "fail")
        self.assertIn("immutable", decision.reason)
        self.assertIn("oldsha", decision.reason)
        self.assertIn("newsha", decision.reason)

    def test_existing_tag_with_successful_release_skips(self) -> None:
        decision = policy.decide_release_tag(
            tag_exists=True,
            tag_sha="abc123",
            target_sha="abc123",
            has_successful_release=True,
            has_active_release=False,
        )
        self.assertEqual(decision.action, "skip")
        self.assertIn("successful", decision.reason)

    def test_existing_tag_with_active_release_skips(self) -> None:
        decision = policy.decide_release_tag(
            tag_exists=True,
            tag_sha="abc123",
            target_sha="abc123",
            has_successful_release=False,
            has_active_release=True,
        )
        self.assertEqual(decision.action, "skip")
        self.assertIn("active", decision.reason)

    def test_failed_release_with_matching_sha_can_retry(self) -> None:
        decision = policy.decide_release_tag(
            tag_exists=True,
            tag_sha="abc123",
            target_sha="abc123",
            has_successful_release=False,
            has_active_release=False,
        )
        self.assertEqual(decision.action, "dispatch")
        self.assertIn("retrying", decision.reason)

    def test_existing_tag_with_unresolved_sha_fails(self) -> None:
        decision = policy.decide_release_tag(
            tag_exists=True,
            tag_sha=None,
            target_sha="abc123",
            has_successful_release=False,
            has_active_release=False,
        )
        self.assertEqual(decision.action, "fail")
        self.assertIn("could not be resolved", decision.reason)

    def test_empty_target_sha_fails(self) -> None:
        decision = policy.decide_release_tag(
            tag_exists=False,
            tag_sha=None,
            target_sha="",
            has_successful_release=False,
            has_active_release=False,
        )
        self.assertEqual(decision.action, "fail")
        self.assertIn("target SHA is empty", decision.reason)

    def test_cli_emits_github_output_for_dispatch(self) -> None:
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            output_path = pathlib.Path(tmp) / "gha_output"
            exit_code = _run_cli(
                "--target-sha", "abc123",
                "--github-output", str(output_path),
            )
            self.assertEqual(exit_code, 0)
            self.assertIn("dispatch=true", output_path.read_text())

    def test_cli_emits_github_output_for_skip(self) -> None:
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            output_path = pathlib.Path(tmp) / "gha_output"
            exit_code = _run_cli(
                "--tag-exists",
                "--tag-sha", "abc123",
                "--target-sha", "abc123",
                "--has-successful-release",
                "--github-output", str(output_path),
            )
            self.assertEqual(exit_code, 0)
            self.assertIn("dispatch=false", output_path.read_text())

    def test_cli_returns_nonzero_for_failure(self) -> None:
        exit_code = _run_cli(
            "--tag-exists",
            "--tag-sha", "oldsha",
            "--target-sha", "newsha",
        )
        self.assertNotEqual(exit_code, 0)


def _run_cli(*args: str) -> int:
    import subprocess

    result = subprocess.run(
        [sys.executable, str(MODULE_PATH), *args],
        capture_output=True,
        text=True,
        check=False,
    )
    return result.returncode


if __name__ == "__main__":
    unittest.main()
