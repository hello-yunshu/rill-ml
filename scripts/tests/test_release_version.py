from __future__ import annotations

import json
import pathlib
import sys
import tempfile
import unittest


SCRIPTS = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from release_version import validate_release  # noqa: E402


STABLE_PLAN = """\
[stable]
version = "1.0.0-rc.1"
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


def _package(name: str, version: str, deps: list | None = None) -> dict:
    return {
        "id": name,
        "name": name,
        "version": version,
        "dependencies": deps or [],
    }


class ReleaseVersionTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temp_dir.name)
        (self.root / "models/example-default").mkdir(parents=True)
        (self.root / "crates/rill-ml-python").mkdir(parents=True)
        (self.root / "release-plan.toml").write_text(STABLE_PLAN, encoding="utf-8")
        self.write_sources("1.0.0-rc.1", "0.13.0")
        self.metadata = {
            "workspace_members": ["rill-ml", "rill-runtime", "rill-ml-python"],
            "packages": [
                _package("rill-ml", "1.0.0-rc.1"),
                _package(
                    "rill-runtime",
                    "1.0.0-rc.1",
                    [
                        {
                            "name": "rill-ml",
                            "path": str(self.root),
                            "req": "^1.0.0-rc.1",
                        }
                    ],
                ),
                _package(
                    "rill-ml-python",
                    "0.13.0",
                    [
                        {
                            "name": "rill-ml",
                            "path": str(self.root),
                            "req": "^1.0.0-rc.1",
                        }
                    ],
                ),
            ],
        }

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def write_sources(self, stable: str, preview: str) -> None:
        (self.root / "models/example-default/manifest.json").write_text(
            json.dumps(
                {"version": stable, "minRuntimeVersion": stable}
            ),
            encoding="utf-8",
        )
        (self.root / "crates/rill-ml-python/pyproject.toml").write_text(
            f'[project]\nname = "rill-ml-python"\nversion = "{preview}"\n',
            encoding="utf-8",
        )
        (self.root / "CHANGELOG.md").write_text(
            f"## [Unreleased]\n\n## [{stable}] - 2026-07-27\n",
            encoding="utf-8",
        )

    def test_accepts_consistent_stable_and_preview_versions(self) -> None:
        self.assertEqual(
            validate_release(self.root, self.metadata), "1.0.0-rc.1"
        )

    def test_rejects_stable_crate_at_wrong_version(self) -> None:
        self.metadata["packages"][0]["version"] = "1.0.0"
        with self.assertRaisesRegex(ValueError, "stable crate rill-ml"):
            validate_release(self.root, self.metadata)

    def test_rejects_preview_crate_at_stable_version(self) -> None:
        self.metadata["packages"][2]["version"] = "1.0.0-rc.1"
        with self.assertRaisesRegex(ValueError, "preview crate rill-ml-python"):
            validate_release(self.root, self.metadata)

    def test_rejects_stale_local_dependency_requirement(self) -> None:
        self.metadata["packages"][1]["dependencies"][0]["req"] = "^0.13.0"
        with self.assertRaisesRegex(
            ValueError, r"expected '\^1\.0\.0-rc\.1'"
        ):
            validate_release(self.root, self.metadata)

    def test_rejects_python_version_drift(self) -> None:
        self.write_sources("1.0.0-rc.1", "0.14.0")
        with self.assertRaisesRegex(ValueError, "expected preview '0.13.0'"):
            validate_release(self.root, self.metadata)

    def test_rejects_manifest_version_drift(self) -> None:
        (self.root / "models/example-default/manifest.json").write_text(
            json.dumps(
                {"version": "0.13.0", "minRuntimeVersion": "1.0.0-rc.1"}
            ),
            encoding="utf-8",
        )
        with self.assertRaisesRegex(ValueError, "manifest.json:version"):
            validate_release(self.root, self.metadata)

    def test_rejects_missing_changelog_release_section(self) -> None:
        (self.root / "CHANGELOG.md").write_text(
            "## [Unreleased]\n", encoding="utf-8"
        )
        with self.assertRaisesRegex(ValueError, "no dated release section"):
            validate_release(self.root, self.metadata)

    def test_rejects_crate_missing_from_release_plan(self) -> None:
        self.metadata["packages"].append(
            _package("mystery-crate", "1.0.0-rc.1")
        )
        self.metadata["workspace_members"].append("mystery-crate")
        with self.assertRaisesRegex(ValueError, "not listed in release-plan.toml"):
            validate_release(self.root, self.metadata)


if __name__ == "__main__":
    unittest.main()
