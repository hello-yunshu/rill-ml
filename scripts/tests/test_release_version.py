import json
import pathlib
import sys
import tempfile
import unittest


SCRIPTS = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from release_version import validate_release  # noqa: E402


class ReleaseVersionTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temp_dir.name)
        (self.root / "models/example-default").mkdir(parents=True)
        (self.root / "crates/rill-ml-python").mkdir(parents=True)
        self.write_sources("0.6.0")
        self.metadata = {
            "workspace_members": ["root", "runtime", "python"],
            "packages": [
                {
                    "id": "root",
                    "name": "rill-ml",
                    "version": "0.6.0",
                    "dependencies": [],
                },
                {
                    "id": "runtime",
                    "name": "rill-runtime",
                    "version": "0.6.0",
                    "dependencies": [
                        {
                            "name": "rill-ml",
                            "path": str(self.root),
                            "req": "^0.6.0",
                        }
                    ],
                },
                {
                    "id": "python",
                    "name": "rill-ml-python",
                    "version": "0.6.0",
                    "dependencies": [
                        {
                            "name": "rill-ml",
                            "path": str(self.root),
                            "req": "^0.6.0",
                        }
                    ],
                },
            ],
        }

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def write_sources(self, version: str) -> None:
        (self.root / "models/example-default/manifest.json").write_text(
            json.dumps({"version": version, "minRuntimeVersion": version}),
            encoding="utf-8",
        )
        (self.root / "crates/rill-ml-python/pyproject.toml").write_text(
            f'[project]\nname = "rill-ml-python"\nversion = "{version}"\n',
            encoding="utf-8",
        )
        (self.root / "CHANGELOG.md").write_text(
            f"## [Unreleased]\n\n## [{version}] - 2026-07-15\n",
            encoding="utf-8",
        )

    def test_accepts_one_consistent_workspace_version(self) -> None:
        self.assertEqual(validate_release(self.root, self.metadata), "0.6.0")

    def test_rejects_workspace_version_disagreement(self) -> None:
        self.metadata["packages"][1]["version"] = "0.5.2"
        with self.assertRaisesRegex(ValueError, "workspace package versions disagree"):
            validate_release(self.root, self.metadata)

    def test_rejects_stale_local_dependency_requirement(self) -> None:
        self.metadata["packages"][1]["dependencies"][0]["req"] = "^0.5.2"
        with self.assertRaisesRegex(ValueError, "expected \^0.6.0"):
            validate_release(self.root, self.metadata)

    def test_rejects_python_or_model_version_drift(self) -> None:
        self.write_sources("0.5.2")
        with self.assertRaisesRegex(ValueError, "manifest.json:version"):
            validate_release(self.root, self.metadata)

    def test_rejects_missing_changelog_release_section(self) -> None:
        (self.root / "CHANGELOG.md").write_text(
            "## [Unreleased]\n", encoding="utf-8"
        )
        with self.assertRaisesRegex(ValueError, "no dated release section"):
            validate_release(self.root, self.metadata)


if __name__ == "__main__":
    unittest.main()
