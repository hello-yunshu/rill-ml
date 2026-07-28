from __future__ import annotations

import pathlib
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import parse_release_plan  # noqa: E402  (path inserted above)
import sync_version  # noqa: E402  (path inserted above)


# Known workspace member names used by the mocked ``workspace_member_names``.
KNOWN_CRATES = {
    "rill-handler-api",
    "rill-runtime-protocol",
    "rill-ml",
    "rill-runtime",
    "rill-ml-python",
    "rill-ml-wasm",
    "rill-ml-tokio",
    "rill-ml-arrow",
    "rill-ml-polars",
    "rillml-inspect",
}


def _write_plan(root: pathlib.Path, content: str) -> None:
    """Write a ``release-plan.toml`` file to ``root``."""
    (root / "release-plan.toml").write_text(content, encoding="utf-8")


NORMAL_PLAN = """\
[stable]
version = "1.0.0-rc.6"
crates = [
  "rill-handler-api",
  "rill-runtime-protocol",
  "rill-ml",
  "rill-runtime",
]

[preview]
version = "0.13.0"
crates = [
  "rill-ml-python",
  "rill-ml-wasm",
]
"""


class ParseReleasePlanTest(unittest.TestCase):
    """Unit tests for ``parse_release_plan.validate_release_plan``."""

    def test_normal_reading_returns_stable_crates(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            _write_plan(temp, NORMAL_PLAN)
            with patch.object(parse_release_plan, "workspace_member_names", return_value=KNOWN_CRATES):
                stable = parse_release_plan.validate_release_plan(temp)
            self.assertEqual(
                stable,
                ["rill-handler-api", "rill-runtime-protocol", "rill-ml", "rill-runtime"],
            )

    def test_stable_order_preserved(self) -> None:
        """The Stable crate list must preserve the file order (publish order)."""
        plan = """\
[stable]
version = "1.0.0-rc.6"
crates = ["rill-ml", "rill-handler-api", "rill-runtime", "rill-runtime-protocol"]

[preview]
version = "0.13.0"
crates = ["rill-ml-python"]
"""
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            _write_plan(temp, plan)
            with patch.object(parse_release_plan, "workspace_member_names", return_value=KNOWN_CRATES):
                stable = parse_release_plan.validate_release_plan(temp)
            self.assertEqual(
                stable,
                ["rill-ml", "rill-handler-api", "rill-runtime", "rill-runtime-protocol"],
            )

    def test_preview_not_in_stable_output(self) -> None:
        """Preview crates must not appear in the Stable return list."""
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            _write_plan(temp, NORMAL_PLAN)
            with patch.object(parse_release_plan, "workspace_member_names", return_value=KNOWN_CRATES):
                stable = parse_release_plan.validate_release_plan(temp)
            for preview_crate in ("rill-ml-python", "rill-ml-wasm"):
                self.assertNotIn(preview_crate, stable)

    def test_duplicate_stable_rejected(self) -> None:
        plan = """\
[stable]
version = "1.0.0-rc.6"
crates = ["rill-ml", "rill-ml", "rill-handler-api"]

[preview]
version = "0.13.0"
crates = ["rill-ml-python"]
"""
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            _write_plan(temp, plan)
            with patch.object(parse_release_plan, "workspace_member_names", return_value=KNOWN_CRATES):
                with self.assertRaisesRegex(RuntimeError, "duplicate crate 'rill-ml'"):
                    parse_release_plan.validate_release_plan(temp)

    def test_duplicate_preview_rejected(self) -> None:
        plan = """\
[stable]
version = "1.0.0-rc.6"
crates = ["rill-ml"]

[preview]
version = "0.13.0"
crates = ["rill-ml-python", "rill-ml-python"]
"""
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            _write_plan(temp, plan)
            with patch.object(parse_release_plan, "workspace_member_names", return_value=KNOWN_CRATES):
                with self.assertRaisesRegex(RuntimeError, "duplicate crate 'rill-ml-python'"):
                    parse_release_plan.validate_release_plan(temp)

    def test_unknown_crate_rejected(self) -> None:
        plan = """\
[stable]
version = "1.0.0-rc.6"
crates = ["rill-ml", "nonexistent-crate"]

[preview]
version = "0.13.0"
crates = ["rill-ml-python"]
"""
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            _write_plan(temp, plan)
            with patch.object(parse_release_plan, "workspace_member_names", return_value=KNOWN_CRATES):
                with self.assertRaisesRegex(RuntimeError, "unknown crate 'nonexistent-crate'"):
                    parse_release_plan.validate_release_plan(temp)

    def test_empty_stable_rejected(self) -> None:
        plan = """\
[stable]
version = "1.0.0-rc.6"
crates = []

[preview]
version = "0.13.0"
crates = ["rill-ml-python"]
"""
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            _write_plan(temp, plan)
            with patch.object(parse_release_plan, "workspace_member_names", return_value=KNOWN_CRATES):
                with self.assertRaisesRegex(RuntimeError, "empty crates list"):
                    parse_release_plan.validate_release_plan(temp)

    def test_version_matches_tag(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            _write_plan(temp, NORMAL_PLAN)
            with patch.object(parse_release_plan, "workspace_member_names", return_value=KNOWN_CRATES):
                stable = parse_release_plan.validate_release_plan(
                    temp, tag_version="1.0.0-rc.6"
                )
            self.assertEqual(len(stable), 4)

    def test_version_mismatch_tag_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            _write_plan(temp, NORMAL_PLAN)
            with patch.object(parse_release_plan, "workspace_member_names", return_value=KNOWN_CRATES):
                with self.assertRaisesRegex(RuntimeError, "does not match"):
                    parse_release_plan.validate_release_plan(
                        temp, tag_version="1.0.0-rc.5"
                    )

    def test_overlap_between_stable_and_preview_rejected(self) -> None:
        plan = """\
[stable]
version = "1.0.0-rc.6"
crates = ["rill-ml", "rill-ml-python"]

[preview]
version = "0.13.0"
crates = ["rill-ml-python", "rill-ml-wasm"]
"""
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            _write_plan(temp, plan)
            with patch.object(parse_release_plan, "workspace_member_names", return_value=KNOWN_CRATES):
                with self.assertRaisesRegex(RuntimeError, "both stable and preview"):
                    parse_release_plan.validate_release_plan(temp)

    def test_invalid_semver_rejected(self) -> None:
        plan = """\
[stable]
version = "not-a-version"
crates = ["rill-ml"]

[preview]
version = "0.13.0"
crates = ["rill-ml-python"]
"""
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            _write_plan(temp, plan)
            with patch.object(parse_release_plan, "workspace_member_names", return_value=KNOWN_CRATES):
                with self.assertRaisesRegex(RuntimeError, "not valid SemVer"):
                    parse_release_plan.validate_release_plan(temp)


if __name__ == "__main__":
    unittest.main()
