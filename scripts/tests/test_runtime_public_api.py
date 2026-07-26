"""Unit tests for ``scripts/check_runtime_public_api.py``.

These tests cover the classification logic that decides whether the
ticker probe is reachable from an external crate.  The actual ``cargo
check`` invocation is intentionally not exercised here — it is an
integration concern owned by the script's ``main()`` and the CI step
that calls it.  The unit tests verify that, given a known cargo
returncode / stderr pair, the classifiers produce the right verdict.
"""

from __future__ import annotations

import importlib.util
import pathlib
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "check_runtime_public_api.py"


def _load_module():
    spec = importlib.util.spec_from_file_location(
        "check_runtime_public_api", MODULE_PATH
    )
    module = importlib.util.module_from_spec(spec)
    sys.modules["check_runtime_public_api"] = module
    spec.loader.exec_module(module)
    return module


check = _load_module()


class CheckRuntimePublicApiTest(unittest.TestCase):
    def test_accepts_normal_public_runtime_api(self) -> None:
        """A smoke crate that compiles (returncode 0) is accepted."""
        self.assertTrue(check.verify_smoke_succeeds(0, ""))
        self.assertFalse(check.verify_smoke_succeeds(1, "error: ..."))

    def test_rejects_ticker_probe_import(self) -> None:
        """Probe crate whose stderr contains a rejection marker is rejected."""
        for marker in check.REJECTION_MARKERS:
            with self.subTest(marker=marker):
                self.assertTrue(
                    check.is_rejection(
                        1, f"error: {marker} `active_epoch_ticker_count`"
                    )
                )
        # returncode 0 means the probe compiled — not rejected.
        self.assertFalse(check.is_rejection(0, ""))
        # Non-zero returncode without a known marker is treated as a
        # suspicious failure (not a clean rejection) so the script
        # surfaces it for human review rather than silently passing.
        self.assertFalse(check.is_rejection(1, "some unrelated error"))

    def test_fails_if_probe_import_compiles(self) -> None:
        """If the probe crate compiles, classify as 'leaked' (public API leak)."""
        self.assertEqual(check.classify_probe_result(0, ""), "leaked")
        self.assertEqual(
            check.classify_probe_result(1, "unresolved import"), "rejected"
        )


if __name__ == "__main__":
    unittest.main()
