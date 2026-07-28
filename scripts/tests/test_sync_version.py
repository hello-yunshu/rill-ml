from __future__ import annotations

import datetime
import pathlib
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import sync_version  # noqa: E402  (path inserted above)


class SyncVersionHelpersTest(unittest.TestCase):
    """Unit tests for the sync_version helper functions."""

    def test_verify_readme_accepts_cargo_add_install(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            readme = temp / "README.md"
            readme.write_text(
                "## Installation\n\n"
                "```bash\n"
                "cargo add rill-ml\n"
                "```\n\n"
                "```bash\n"
                "cargo add rill-ml --features serde\n"
                "```\n",
                encoding="utf-8",
            )
            self.assertEqual(sync_version.verify_readme_no_hardcoded_version(readme), 0)

    def test_verify_readme_detects_plain_toml_dependency(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            readme = temp / "README.md"
            readme.write_text(
                "## Installation\n\n"
                "```toml\n"
                "[dependencies]\n"
                'rill-ml = "0.7"\n'
                "```\n",
                encoding="utf-8",
            )
            self.assertEqual(sync_version.verify_readme_no_hardcoded_version(readme), 1)

    def test_verify_readme_detects_feature_dependency(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            readme = temp / "README.md"
            readme.write_text(
                "```toml\n"
                "[dependencies]\n"
                'rill-ml = { version = "0.7", features = ["serde"] }\n'
                "```\n",
                encoding="utf-8",
            )
            # The regex matches the leading ``rill-ml =`` portion; the
            # inline table form counts as one violation.
            self.assertEqual(sync_version.verify_readme_no_hardcoded_version(readme), 1)

    def test_verify_readme_ignores_roadmap_bullets(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            readme = temp / "README.md"
            readme.write_text(
                "## Roadmap\n\n"
                "- **v0.1** — Basic closed loop.\n"
                "- **v0.7** — Pluggable WASM handlers. *(current)*\n"
                "- **v1.0** — Stable API.\n",
                encoding="utf-8",
            )
            self.assertEqual(sync_version.verify_readme_no_hardcoded_version(readme), 0)

    def test_verify_readme_counts_multiple_violations(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            readme = temp / "README.md"
            readme.write_text(
                'rill-ml = "0.7"\n'
                'rill-ml = "0.8"\n'
                'rill-ml = { version = "0.7", features = ["serde"] }\n',
                encoding="utf-8",
            )
            self.assertEqual(sync_version.verify_readme_no_hardcoded_version(readme), 3)

    def test_sync_workspace_deps_is_idempotent(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            cargo_toml = temp / "Cargo.toml"
            original = (
                "[workspace.package]\n"
                'version = "0.8.1"\n'
                "\n"
                "[workspace.dependencies]\n"
                'rill-ml = { version = "0.8.1", path = "." }\n'
                'rill-runtime-protocol = { version = "0.8.1", path = "crates/rill-runtime-protocol" }\n'
            )
            cargo_toml.write_text(original, encoding="utf-8")
            first = sync_version.sync_workspace_deps(cargo_toml, "0.8.1")
            second = sync_version.sync_workspace_deps(cargo_toml, "0.8.1")
            self.assertEqual(first, 0, "already-up-to date should report 0")
            self.assertEqual(second, 0, "second run must not change anything")
            self.assertEqual(cargo_toml.read_text(encoding="utf-8"), original)

    def test_sync_workspace_deps_updates_old_version(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            cargo_toml = temp / "Cargo.toml"
            cargo_toml.write_text(
                "[workspace.dependencies]\n"
                'rill-ml = { version = "0.7.0", path = "." }\n',
                encoding="utf-8",
            )
            count = sync_version.sync_workspace_deps(cargo_toml, "0.8.1")
            self.assertEqual(count, 1)
            self.assertIn('version = "0.8.1"', cargo_toml.read_text(encoding="utf-8"))

    def test_sync_pyproject_is_idempotent(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            pyproject = temp / "pyproject.toml"
            pyproject.write_text(
                '[project]\nname = "rill-ml-python"\nversion = "0.8.1"\n',
                encoding="utf-8",
            )
            self.assertEqual(sync_version.sync_pyproject(pyproject, "0.8.1"), 0)
            self.assertEqual(sync_version.sync_pyproject(pyproject, "0.8.1"), 0)

    def test_sync_lock_package_version_updates_only_named_package(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            cargo_lock = pathlib.Path(temp_name) / "Cargo.lock"
            cargo_lock.write_text(
                'version = 4\n\n'
                '[[package]]\nname = "echo-handler"\nversion = "1.0.0-rc.6"\n\n'
                '[[package]]\nname = "dependency"\nversion = "1.0.0-rc.6"\n',
                encoding="utf-8",
            )
            self.assertEqual(
                sync_version.sync_lock_package_version(
                    cargo_lock, "echo-handler", "1.0.0"
                ),
                1,
            )
            text = cargo_lock.read_text(encoding="utf-8")
            self.assertIn('name = "echo-handler"\nversion = "1.0.0"', text)
            self.assertIn('name = "dependency"\nversion = "1.0.0-rc.6"', text)
            self.assertEqual(
                sync_version.sync_lock_package_version(
                    cargo_lock, "echo-handler", "1.0.0"
                ),
                0,
            )

    def test_sync_roadmap_is_idempotent(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            roadmap = temp / "ROADMAP.md"
            today = datetime.date.today().isoformat()
            roadmap.write_text(
                "# Roadmap\n\n"
                f"> 状态：当前（v0.8.1，{today}）\n",
                encoding="utf-8",
            )
            self.assertEqual(sync_version.sync_roadmap(roadmap, "0.8.1"), 0)
            self.assertEqual(sync_version.sync_roadmap(roadmap, "0.8.1"), 0)

    def test_sync_changelog_is_idempotent(self) -> None:
        with tempfile.TemporaryDirectory() as temp_name:
            temp = pathlib.Path(temp_name)
            changelog = temp / "CHANGELOG.md"
            today = datetime.date.today().isoformat()
            changelog.write_text(
                "# Changelog\n\n"
                "## [Unreleased]\n\n"
                f"## [0.8.1] - {today}\n\n"
                "### Changed\n\n"
                "- describe changes.\n\n"
                "[Unreleased]: https://github.com/hello-yunshu/rill-ml/compare/v0.8.1...HEAD\n"
                "[0.8.1]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.8.1\n",
                encoding="utf-8",
            )
            self.assertEqual(sync_version.sync_changelog(changelog, "0.8.1"), 0)
            self.assertEqual(sync_version.sync_changelog(changelog, "0.8.1"), 0)


class SyncVersionReadmeRegressionTest(unittest.TestCase):
    """Ensure the tracked README files do not regress to hardcoded versions."""

    def test_readme_md_has_no_hardcoded_rill_ml_version(self) -> None:
        readme = ROOT / "README.md"
        self.assertTrue(readme.exists(), "README.md must exist at repo root")
        self.assertEqual(sync_version.verify_readme_no_hardcoded_version(readme), 0)

    def test_readme_en_has_no_hardcoded_rill_ml_version(self) -> None:
        readme = ROOT / "README.en.md"
        self.assertTrue(readme.exists(), "README.en.md must exist at repo root")
        self.assertEqual(sync_version.verify_readme_no_hardcoded_version(readme), 0)

    def test_readme_md_uses_cargo_add_install(self) -> None:
        text = (ROOT / "README.md").read_text(encoding="utf-8")
        self.assertIn("cargo add rill-ml", text)

    def test_readme_en_uses_cargo_add_install(self) -> None:
        text = (ROOT / "README.en.md").read_text(encoding="utf-8")
        self.assertIn("cargo add rill-ml", text)


if __name__ == "__main__":
    unittest.main()
