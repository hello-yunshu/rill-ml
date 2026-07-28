import importlib.util
import pathlib
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "check_state_fixture_coverage.py"
SPEC = importlib.util.spec_from_file_location("check_state_fixture_coverage", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class StateFixtureCoverageTests(unittest.TestCase):
    def test_repository_manifest_fixtures_tests_and_docs_match(self) -> None:
        self.assertEqual(MODULE.validate_coverage(ROOT), [])

    def test_document_parser_reads_only_schema_sections(self) -> None:
        content = """\
# Policy

### Stable state schema types

| Type | Fixture |
|---|---|
| `Mean` | `mean` |
| `Variance` | `variance` |

### Preview state schema types

- `Adwin` (detector)
- `Kswin`

### State schema manifest

Elsewhere: `NotAStateType`.
"""
        with tempfile.TemporaryDirectory() as temp_dir:
            path = pathlib.Path(temp_dir) / "STABILITY.md"
            path.write_text(content, encoding="utf-8")
            stable, preview = MODULE.parse_documented_types(path)
        self.assertEqual(stable, ["Mean", "Variance"])
        self.assertEqual(preview, ["Adwin", "Kswin"])

    def test_manifest_parser_uses_real_toml_rules(self) -> None:
        content = """\
[[stable_state]]
type = "Mean"
fixture = "mean"

[[preview_state]]
type = "Adwin"
reason = "preview"
"""
        with tempfile.TemporaryDirectory() as temp_dir:
            path = pathlib.Path(temp_dir) / "state-schema-manifest.toml"
            path.write_text(content, encoding="utf-8")
            stable, preview = MODULE.parse_manifest(path)
        self.assertEqual(stable[0]["fixture"], "mean")
        self.assertEqual(preview[0]["type"], "Adwin")


if __name__ == "__main__":
    unittest.main()
