import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    "dependency_audit", Path(__file__).resolve().parents[1] / "runtime_dependency_audit.py"
)
dependency_audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dependency_audit)

LOCK = """version = 3

[[package]]
name = "rill-runtime"
version = "1.0.0"
dependencies = [
 "serde",
 "windows-sys",
]

[[package]]
name = "serde"
version = "1.0.0"

[[package]]
name = "windows-sys"
version = "0.1.0"
dependencies = [
 "windows-targets",
]

[[package]]
name = "windows-targets"
version = "0.1.0"

[[package]]
name = "polars"
version = "0.46.0"
dependencies = [
 "zstd",
]

[[package]]
name = "zstd"
version = "0.13.3"
"""


class ShippedRuntimeAuditScope(unittest.TestCase):
    def test_closure_keeps_runtime_and_target_specific_dependencies(self):
        filtered = dependency_audit.shipped_runtime_lock(LOCK, {("rill-runtime", "1.0.0")})
        self.assertIn('name = "serde"', filtered)
        self.assertIn('name = "windows-targets"', filtered)
        self.assertNotIn('name = "polars"', filtered)
        self.assertNotIn('name = "zstd"', filtered)

    def test_missing_graph_package_fails_closed(self):
        with self.assertRaisesRegex(ValueError, "missing from Cargo.lock"):
            dependency_audit.shipped_runtime_lock(LOCK, {("rill-runtime", "2.0.0")})


if __name__ == "__main__":
    unittest.main()
