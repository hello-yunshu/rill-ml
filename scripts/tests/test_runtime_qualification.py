import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class RuntimeQualificationRegistryTests(unittest.TestCase):
    def test_fault_registry_is_unique_and_deterministic(self):
        registry = json.loads((ROOT / "schemas/runtime-fault-scenarios-v1.json").read_text())
        scenarios = registry["scenarios"]
        ids = [item["id"] for item in scenarios]
        seeds = [item["seed"] for item in scenarios]
        self.assertEqual(registry["schemaVersion"], 1)
        self.assertEqual(len(ids), len(set(ids)))
        self.assertEqual(len(seeds), len(set(seeds)))
        self.assertTrue(all(item["expected"] for item in scenarios))


if __name__ == "__main__":
    unittest.main()
