"""Unit tests for ``scripts/release_admission.py``.

The release-admission gate is the pipeline's final authority for every release
(including manual ``workflow_dispatch``). These tests cover the pure decision
logic: same-SHA CI/Cross-Platform/Security gates, the §26 "docs-only" rule for
a missing Cross-Platform run, and §29/§30 successful-release immutability.
"""

from __future__ import annotations

import importlib.util
import pathlib
import sys
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "release_admission.py"


def _load_module():
    spec = importlib.util.spec_from_file_location("release_admission", MODULE_PATH)
    module = importlib.util.module_from_spec(spec)
    sys.modules["release_admission"] = module
    spec.loader.exec_module(module)
    return module


admission = _load_module()


def _admit(**overrides):
    defaults = {
        "tag_sha": "abc123",
        "ci_status": "success",
        "cross_status": "success",
        "cross_relevant": True,
        "security_status": "success",
        "has_successful_release": False,
        "has_active_release": False,
    }
    defaults.update(overrides)
    return admission.evaluate_admission(**defaults)


class ClassifyCrossPlatformRelevanceTest(unittest.TestCase):
    def test_cargo_change_is_relevant(self) -> None:
        self.assertTrue(admission.classify_cross_platform_relevance(["Cargo.toml"]))
        self.assertTrue(
            admission.classify_cross_platform_relevance(["Cargo.lock"])
        )
        self.assertTrue(
            admission.classify_cross_platform_relevance(
                ["crates/rill-runtime/src/lib.rs"]
            )
        )

    def test_workflow_and_release_config_are_relevant(self) -> None:
        self.assertTrue(
            admission.classify_cross_platform_relevance(
                [".github/workflows/pipeline.yml"]
            )
        )
        self.assertTrue(
            admission.classify_cross_platform_relevance(["release-plan.toml"])
        )

    def test_scripts_change_is_relevant(self) -> None:
        self.assertTrue(
            admission.classify_cross_platform_relevance(["scripts/qemu-cross-gate.sh"])
        )

    def test_doc_only_is_not_relevant(self) -> None:
        self.assertFalse(
            admission.classify_cross_platform_relevance(
                ["CHANGELOG.md", "README.md", "docs/guide.md"]
            )
        )


class EvaluateAdmissionTest(unittest.TestCase):
    def test_all_gates_pass_admits(self) -> None:
        decision = _admit()
        self.assertTrue(decision.admitted)
        self.assertEqual(decision.reasons, [])

    def test_doc_only_allows_missing_cross_platform(self) -> None:
        # A genuinely docs-only commit may admit a missing Cross-Platform run.
        decision = _admit(cross_status="missing", cross_relevant=False)
        self.assertTrue(decision.admitted)

    def test_relevant_path_with_missing_cross_platform_fails(self) -> None:
        # §26: never guess doc-only when cross-platform-relevant paths changed.
        decision = _admit(cross_status="missing", cross_relevant=True)
        self.assertFalse(decision.admitted)
        self.assertTrue(any("Cross-Platform" in r for r in decision.reasons))

    def test_cross_platform_failure_fails(self) -> None:
        decision = _admit(cross_status="failure")
        self.assertFalse(decision.admitted)

    def test_ci_failure_fails(self) -> None:
        decision = _admit(ci_status="failure")
        self.assertFalse(decision.admitted)

    def test_ci_missing_fails(self) -> None:
        decision = _admit(ci_status="missing")
        self.assertFalse(decision.admitted)

    def test_security_in_progress_fails(self) -> None:
        decision = _admit(security_status="in_progress")
        self.assertFalse(decision.admitted)

    def test_security_missing_fails(self) -> None:
        decision = _admit(security_status="missing")
        self.assertFalse(decision.admitted)

    def test_successful_release_blocks_and_marks_immutable(self) -> None:
        # §29/§30: successful release assets are immutable — a re-dispatch
        # must never mutate them.
        decision = _admit(has_successful_release=True)
        self.assertFalse(decision.admitted)
        self.assertTrue(any("immutable" in r for r in decision.reasons))

    def test_active_release_blocks_concurrent_release(self) -> None:
        decision = _admit(has_active_release=True)
        self.assertFalse(decision.admitted)
        self.assertTrue(any("active" in r for r in decision.reasons))


class AggregateRunStatusTest(unittest.TestCase):
    def test_no_runs_is_missing(self) -> None:
        self.assertEqual(admission._aggregate_run_status([]), "missing")

    def test_success_is_success(self) -> None:
        runs = [{"status": "completed", "conclusion": "success"}]
        self.assertEqual(admission._aggregate_run_status(runs), "success")

    def test_pending_is_in_progress(self) -> None:
        runs = [{"status": "in_progress", "conclusion": None}]
        self.assertEqual(admission._aggregate_run_status(runs), "in_progress")

    def test_failed_only_is_failed(self) -> None:
        runs = [{"status": "completed", "conclusion": "failure"}]
        self.assertEqual(admission._aggregate_run_status(runs), "failed")

    def test_success_beats_earlier_failure(self) -> None:
        # A later completed+success run on the same SHA supersedes an earlier
        # failure (proves the code passes), so the gate is satisfied.
        runs = [
            {"status": "completed", "conclusion": "failure"},
            {"status": "completed", "conclusion": "success"},
        ]
        self.assertEqual(admission._aggregate_run_status(runs), "success")


if __name__ == "__main__":
    unittest.main()
