import json
import sys
import unittest
from unittest.mock import patch
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import run_runtime_final_qualification as qualification  # noqa: E402


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

    @staticmethod
    def _phase(mode="simulated-consumer"):
        return {
            "status": "PASS",
            "mode": mode,
            "decisionLatencySeconds": [1.0],
            "feedbackLatencySeconds": [2.0],
        }

    def test_resource_cap_failure_fails_overall(self):
        with patch.object(qualification, "_phase", return_value=self._phase()), patch.object(
            qualification, "_resource_cap_probe", return_value={"status": "FAIL"}
        ), patch.object(
            qualification, "_continuous_same_state_saturation", return_value={"status": "PASS"}
        ):
            result = qualification.qualify(
                Path("runtime"), 1, ["simulated-consumer"], 1, 0, "simulated-consumer"
            )
        self.assertEqual(result["status"], "FAIL")

    def test_resource_cap_is_required_without_soak(self):
        with patch.object(qualification, "_phase", return_value=self._phase()), patch.object(
            qualification, "_resource_cap_probe", return_value={"status": "FAIL"}
        ), patch.object(
            qualification, "_continuous_same_state_saturation", return_value={"status": "PASS"}
        ):
            result = qualification.qualify(
                Path("runtime"), 1, ["simulated-consumer"], 1, 0, "simulated-consumer"
            )
        self.assertIn({"checkId": "resource-cap-probe", "status": "FAIL"}, result["requiredChecks"])

    def test_soak_failure_fails_overall(self):
        with patch.object(qualification, "_phase", return_value=self._phase()), patch.object(
            qualification, "_resource_cap_probe", return_value={"status": "PASS"}
        ), patch.object(qualification, "_bounded_soak", return_value={"status": "FAIL"}), patch.object(
            qualification, "_continuous_same_state_saturation", return_value={"status": "PASS"}
        ):
            result = qualification.qualify(
                Path("runtime"), 1, ["simulated-consumer"], 1, 1, "simulated-consumer"
            )
        self.assertEqual(result["status"], "FAIL")

    def test_phase_failure_fails_overall(self):
        with patch.object(qualification, "_phase", return_value={**self._phase(), "status": "FAIL"}), patch.object(
            qualification, "_resource_cap_probe", return_value={"status": "PASS"}
        ), patch.object(
            qualification, "_continuous_same_state_saturation", return_value={"status": "PASS"}
        ):
            result = qualification.qualify(
                Path("runtime"), 1, ["simulated-consumer"], 1, 0, "simulated-consumer"
            )
        self.assertEqual(result["status"], "FAIL")

    def test_percentile_math(self):
        self.assertEqual(qualification._percentile([1.0, 2.0, 3.0], 0.5), 2.0)
        self.assertEqual(qualification._percentile([], 0.99), 0.0)

    def test_invalid_batch_size_fails(self):
        with self.assertRaises(ValueError):
            qualification.qualify(Path("runtime"), 1, ["simulated-consumer"], 0, 0, "simulated-consumer")

    def test_invalid_observation_count_fails(self):
        with self.assertRaises(ValueError):
            qualification.qualify(Path("runtime"), 0, ["simulated-consumer"], 1, 0, "simulated-consumer")

    def test_output_schema_is_stable(self):
        with patch.object(qualification, "_phase", return_value=self._phase()), patch.object(
            qualification, "_resource_cap_probe", return_value={"status": "PASS"}
        ), patch.object(
            qualification, "_continuous_same_state_saturation", return_value={"status": "PASS"}
        ):
            result = qualification.qualify(
                Path("runtime"), 1, ["simulated-consumer"], 1, 0, "simulated-consumer"
            )
        self.assertEqual(result["schemaVersion"], qualification.SCHEMA_VERSION)
        self.assertEqual(result["evidenceType"], "simulated")
        self.assertIn("requiredChecks", result)


if __name__ == "__main__":
    unittest.main()
