#!/usr/bin/env python3
"""Run post-push synthetic consumer qualification for Preview v3.

This runner is separate from the bounded PR smoke and is intended for the
final qualification stage after 1.5 code and normal Actions pass. Its output
is explicitly simulated evidence.
"""

from __future__ import annotations

import argparse
import json
import statistics
import tempfile
import time
from pathlib import Path

from run_runtime_qualification import envelope, run_process


SCHEMA_VERSION = 1


def _phase(runtime: Path, mode: str, observations: int) -> dict:
    with tempfile.TemporaryDirectory(prefix=f"rill-{mode}-") as directory:
        state = Path(directory) / "runtime-state.json"
        requests = [envelope(f"{mode}-handshake", None, 0, {"method": "handshake"})]
        requests += [
            envelope(
                f"{mode}-decision-{index}",
                "org.rill.preview.decide",
                index,
                {
                    "method": "decide",
                    "context": {
                        "consumer": mode,
                        "features": [float(index % 17), float(index % 5)],
                    },
                },
            )
            for index in range(observations)
        ]
        start = time.perf_counter()
        first = run_process(runtime, state, requests)
        decision_elapsed = time.perf_counter() - start
        if first[0]["response"].get("channel") != "preview":
            raise RuntimeError(f"{mode}: preview channel was not advertised")
        accepted = [item for item in first[1:] if item["response"].get("kind") == "result"]
        if len(accepted) != observations:
            raise RuntimeError(f"{mode}: accepted {len(accepted)} of {observations} decisions")

        feedback = [
            envelope(
                f"{mode}-feedback-{index}",
                "org.rill.preview.feedback",
                observations + index,
                {
                    "method": "feedback",
                    "decisionId": f"{mode}-decision-{index}",
                    "selectedArm": index % 2,
                    "reward": 1.0 if index % 3 else 0.0,
                    "outcomeTimeMs": index + 1,
                    "generation": 0,
                },
            )
            for index in range(observations)
        ]
        start = time.perf_counter()
        second = run_process(runtime, state, feedback)
        feedback_elapsed = time.perf_counter() - start
        if any(item["response"].get("kind") != "result" for item in second):
            raise RuntimeError(f"{mode}: feedback after restart was rejected")

        inspect = run_process(
            runtime,
            state,
            [
                envelope(
                    f"{mode}-inspect",
                    "org.rill.preview.inspect",
                    observations * 2,
                    {"method": "inspect"},
                )
            ],
        )
        summary = inspect[0]["response"].get("summary", {})
        return {
            "mode": mode,
            "observations": observations,
            "decisionsAccepted": len(accepted),
            "feedbackAccepted": len(second),
            "pendingDecisions": summary.get("pendingDecisions"),
            "completedDecisions": summary.get("completedDecisions"),
            "decisionSeconds": decision_elapsed,
            "feedbackSeconds": feedback_elapsed,
            "stateGeneration": inspect[0].get("stateGeneration"),
        }


def qualify(runtime: Path, observations: int, modes: list[str]) -> dict:
    if observations < 1 or observations > 1_000_000:
        raise ValueError("observations must be between 1 and 1000000")
    phases = [_phase(runtime, mode, observations) for mode in modes]
    decision_rates = [phase["decisionSeconds"] / phase["observations"] for phase in phases]
    feedback_rates = [phase["feedbackSeconds"] / phase["observations"] for phase in phases]
    return {
        "schemaVersion": SCHEMA_VERSION,
        "status": "PASS",
        "mode": "synthetic-consumer-qualification",
        "evidenceType": "simulated",
        "consumerQualification": "simulated-only",
        "phases": phases,
        "benchmark": {
            "decisionSecondsPerObservationP50": statistics.median(decision_rates),
            "feedbackSecondsPerObservationP50": statistics.median(feedback_rates),
            "observationsPerPhase": observations,
        },
        "evidence": [
            "real-child-process",
            "preview-handshake",
            "restart-persistence",
            "decision-outcome-feedback",
            "consumer-host-owned-action-boundary",
            "structured-inspection",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--observations", type=int, default=1000)
    parser.add_argument(
        "--mode",
        action="append",
        choices=("simulated-consumer", "pm-style", "network-style"),
        dest="modes",
        help="phase to execute; repeat to select several (default: all)",
    )
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    modes = args.modes or ["simulated-consumer", "pm-style", "network-style"]
    try:
        result = qualify(args.runtime, args.observations, modes)
    except (OSError, ValueError, RuntimeError, json.JSONDecodeError) as error:
        result = {
            "schemaVersion": SCHEMA_VERSION,
            "status": "FAIL",
            "mode": "synthetic-consumer-qualification",
            "evidenceType": "simulated",
            "consumerQualification": "simulated-only",
            "error": str(error),
        }
    print(json.dumps(result, ensure_ascii=False, sort_keys=True, separators=(",", ":") if args.json else None))
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
