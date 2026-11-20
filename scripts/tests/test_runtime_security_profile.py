import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    "profile", Path(__file__).resolve().parents[1] / "check_runtime_security_profile.py"
)
profile = importlib.util.module_from_spec(spec)
spec.loader.exec_module(profile)


class SecurityProfile(unittest.TestCase):
    def test_resolved_engine_retains_default_wasm(self):
        result = profile.assess(
            "rill-runtime v1|wasm\n"
            "wasmtime v99.2.3|component-model,cranelift,pulley,runtime,std\n"
        )
        self.assertIn("runtime", result["features"])

    def test_rejects_silent_async_union_or_removed_wasm(self):
        for value in [
            "",
            "wasmtime v99|runtime",
            "wasmtime v99|component-model,cranelift,pulley,runtime,component-model-async",
            "wasmtime v99|component-model,cranelift,pulley,runtime\n"
            "wasmtime v98|component-model,cranelift,pulley,runtime",
        ]:
            with self.subTest(tree=value), self.assertRaises(ValueError):
                profile.assess(value)


if __name__ == "__main__":
    unittest.main()
