from __future__ import annotations

import sys
import pathlib
import unittest

SCRIPTS = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import release_version_compare as rvc  # noqa: E402


class ReleaseVersionCompareTest(unittest.TestCase):
    def test_stable_beats_prerelease(self) -> None:
        self.assertEqual(rvc.compare("1.0.0", "1.0.0-rc.1"), 1)
        self.assertEqual(rvc.compare("1.0.0-rc.1", "1.0.0"), -1)

    def test_prerelease_numeric_ordering(self) -> None:
        self.assertEqual(rvc.compare("1.0.0-rc.2", "1.0.0-rc.1"), 1)
        self.assertEqual(rvc.compare("1.0.0-rc.10", "1.0.0-rc.9"), 1)

    def test_prerelease_alphanumeric_ordering(self) -> None:
        # alpha < beta < rc (alphanumeric comparison)
        self.assertEqual(rvc.compare("1.0.0-beta", "1.0.0-alpha"), 1)
        self.assertEqual(rvc.compare("1.0.0-rc.1", "1.0.0-beta.2"), 1)

    def test_numeric_field_lower_than_alphanumeric(self) -> None:
        # numeric identifiers always have lower precedence than non-numeric
        self.assertEqual(rvc.compare("1.0.0-alpha", "1.0.0-1"), 1)

    def test_longer_prerelease_wins_when_prefix_equal(self) -> None:
        self.assertEqual(rvc.compare("1.0.0-alpha.1", "1.0.0-alpha"), 1)
        self.assertEqual(rvc.compare("1.0.0-alpha", "1.0.0-alpha.1"), -1)

    def test_release_candidate_is_newer_than_previous_stable(self) -> None:
        # The actual Auto Release use case: 1.0.0-rc.1 vs 0.13.0
        self.assertEqual(rvc.compare("1.0.0-rc.1", "0.13.0"), 1)
        self.assertEqual(rvc.compare("1.0.0", "0.13.0"), 1)

    def test_same_version_is_not_strictly_newer(self) -> None:
        self.assertEqual(rvc.compare("1.0.0", "1.0.0"), 0)
        self.assertEqual(rvc.compare("1.0.0-rc.1", "1.0.0-rc.1"), 0)

    def test_rejects_invalid_semver(self) -> None:
        with self.assertRaises(ValueError):
            rvc.parse("1.0")
        with self.assertRaises(ValueError):
            rvc.parse("1.0.0.0")
        with self.assertRaises(ValueError):
            rvc.parse("v1.0.0")

    def test_cli_returns_zero_for_newer(self) -> None:
        original_argv = sys.argv
        try:
            sys.argv = ["release_version_compare.py", "1.0.0-rc.1", "0.13.0"]
            self.assertEqual(rvc.main(), 0)
        finally:
            sys.argv = original_argv

    def test_cli_returns_one_for_not_newer(self) -> None:
        original_argv = sys.argv
        try:
            sys.argv = ["release_version_compare.py", "0.13.0", "1.0.0-rc.1"]
            self.assertEqual(rvc.main(), 1)
        finally:
            sys.argv = original_argv


if __name__ == "__main__":
    unittest.main()
