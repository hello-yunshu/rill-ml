#!/usr/bin/env python3
"""Run post-push synthetic consumer qualification for Preview v3.

This runner is explicit about its evidence boundary: it starts the real
runtime child process, but its consumers are repository-owned simulations.
The final stage collects latency tails and bounded-load data; it is not a
native platform or external-consumer qualification.
"""

from __future__ import annotations

import argparse
import json
import resource
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from run_runtime_qualification import envelope, run_process


SCHEMA_VERSION = 2
DEFAULT_BATCH_SIZE = 128


def _percentile(values: list[float], percentile: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    position = (len(ordered) - 1) * percentile
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    weight = position - lower
    return ordered[lower] + (ordered[upper] - ordered[lower]) * weight


def _child_max_rss_bytes() -> int:
    value = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    # macOS reports bytes; Linux and the BSDs report KiB.
    return int(value if sys.platform == "darwin" else value * 1024)


def _timed_run(runtime: Path, state: Path, requests: list[dict]) -> tuple[list[dict], float, int]:
    start = time.perf_counter()
    responses = run_process(runtime, state, requests)
    return responses, time.perf_counter() - start, _child_max_rss_bytes()


def _decision_requests(mode: str, start: int, count: int, state_generation: int) -> list[dict]:
    return [
        envelope(
            f"{mode}-decision-{index}",
            "org.rill.preview.decide",
            state_generation + (index - start),
            {
                "method": "decide",
                "context": {
                    "consumer": mode,
                    "features": [float(index % 17), float(index % 5)],
                },
            },
        )
        for index in range(start, start + count)
    ]


def _feedback_requests(mode: str, start: int, count: int, state_generation: int) -> list[dict]:
    return [
        envelope(
            f"{mode}-feedback-{index}",
            "org.rill.preview.feedback",
            state_generation + (index - start),
            {
                "method": "feedback",
                "decisionId": f"{mode}-decision-{index}",
                "selectedArm": index % 2,
                "reward": 1.0 if index % 3 else 0.0,
                "outcomeTimeMs": index + 1,
                "generation": 0,
            },
        )
        for index in range(start, start + count)
    ]


def _phase(runtime: Path, mode: str, observations: int, batch_size: int) -> dict:
    with tempfile.TemporaryDirectory(prefix=f"rill-{mode}-") as directory:
        state = Path(directory) / "runtime-state.json"
        cold_start, cold_elapsed, max_rss = _timed_run(
            runtime,
            state,
            [envelope(f"{mode}-cold-start", None, 0, {"method": "handshake"})],
        )
        if cold_start[0]["response"].get("channel") != "preview":
            raise RuntimeError(f"{mode}: preview channel was not advertised")

        decision_seconds: list[float] = []
        feedback_seconds: list[float] = []
        state_generation = 0
        accepted = 0
        for start in range(0, observations, batch_size):
            count = min(batch_size, observations - start)
            responses, elapsed, max_rss = _timed_run(
                runtime, state, _decision_requests(mode, start, count, state_generation)
            )
            results = [item for item in responses if item["response"].get("kind") == "result"]
            if len(results) != count:
                raise RuntimeError(f"{mode}: accepted {len(results)} of {count} decisions")
            decision_seconds.append(elapsed / count)
            accepted += count
            state_generation += count

        state_bytes_after_decisions = state.stat().st_size
        restore, restore_elapsed, max_rss = _timed_run(
            runtime,
            state,
            [envelope(f"{mode}-restore-inspect", "org.rill.preview.inspect", state_generation, {"method": "inspect"})],
        )
        if restore[0]["response"].get("kind") != "inspection":
            raise RuntimeError(f"{mode}: restart inspect was rejected")
        state_generation += 1

        feedback_accepted = 0
        for start in range(0, observations, batch_size):
            count = min(batch_size, observations - start)
            responses, elapsed, max_rss = _timed_run(
                runtime, state, _feedback_requests(mode, start, count, state_generation)
            )
            if any(item["response"].get("kind") != "result" for item in responses):
                raise RuntimeError(f"{mode}: feedback after restart was rejected")
            feedback_seconds.append(elapsed / count)
            feedback_accepted += count
            state_generation += count

        state_bytes_after_feedback = state.stat().st_size
        snapshot, snapshot_elapsed, max_rss = _timed_run(
            runtime,
            state,
            [envelope(f"{mode}-snapshot", "org.rill.preview.snapshot", state_generation, {"method": "snapshot"})],
        )
        if snapshot[0]["response"].get("kind") != "snapshot":
            raise RuntimeError(f"{mode}: snapshot was rejected")

        inspect, inspect_elapsed, max_rss = _timed_run(
            runtime,
            state,
            [envelope(f"{mode}-inspect", "org.rill.preview.inspect", state_generation, {"method": "inspect"})],
        )
        summary = inspect[0]["response"].get("summary", {})
        if summary.get("pendingDecisions") != 0 or summary.get("completedDecisions") != observations:
            raise RuntimeError(f"{mode}: ledger summary did not converge: {summary}")
        return {
            "mode": mode,
            "observations": observations,
            "batchSize": batch_size,
            "decisionsAccepted": accepted,
            "feedbackAccepted": feedback_accepted,
            "pendingDecisions": summary.get("pendingDecisions"),
            "completedDecisions": summary.get("completedDecisions"),
            "decisionSecondsPerObservation": decision_seconds,
            "feedbackSecondsPerObservation": feedback_seconds,
            "coldStartSeconds": cold_elapsed,
            "restoreSeconds": restore_elapsed,
            "snapshotSeconds": snapshot_elapsed,
            "inspectSeconds": inspect_elapsed,
            "stateBytesAfterDecisions": state_bytes_after_decisions,
            "stateBytesAfterFeedback": state_bytes_after_feedback,
            "stateGeneration": inspect[0].get("stateGeneration"),
            "childMaxRssBytes": max_rss,
        }


def _resource_cap_probe(runtime: Path) -> dict:
    """Verify pending-cap rejection happens before unbounded accumulation."""
    with tempfile.TemporaryDirectory(prefix="rill-capacity-") as directory:
        state = Path(directory) / "runtime-state.json"
        responses = run_process(runtime, state, _decision_requests("capacity", 0, 1_025, 0))
        errors = [item["response"].get("error", {}) for item in responses]
        rejected = [error for error in errors if "capacity" in error.get("message", "")]
        accepted = [item for item in responses if item["response"].get("kind") == "result"]
        inspect = run_process(
            runtime,
            state,
            [envelope("capacity-inspect", "org.rill.preview.inspect", 1_024, {"method": "inspect"})],
        )
        summary = inspect[0]["response"].get("summary", {})
        passed = len(rejected) == 1 and summary.get("pendingDecisions") == 1_024
        return {
            "status": "PASS" if passed else "FAIL",
            "acceptedBeforeRejection": len(accepted),
            "rejectedRequests": len(rejected),
            "pendingDecisionsAfterProbe": summary.get("pendingDecisions"),
            "evidenceType": "simulated",
        }


def _bounded_soak(runtime: Path, mode: str, cycles: int, observations: int, batch_size: int) -> dict:
    samples = [_phase(runtime, f"{mode}-soak-{cycle}", observations, batch_size) for cycle in range(cycles)]
    sizes = [sample["stateBytesAfterFeedback"] for sample in samples]
    rss = [sample["childMaxRssBytes"] for sample in samples]
    return {
        "status": "PASS" if max(sizes) < 512 * 1024 and max(rss) < 512 * 1024 * 1024 else "FAIL",
        "evidenceType": "simulated",
        "mode": mode,
        "cycles": cycles,
        "observationsPerCycle": observations,
        "totalObservations": cycles * observations,
        "stateBytes": {"min": min(sizes), "max": max(sizes), "samples": sizes},
        "childMaxRssBytes": {"min": min(rss), "max": max(rss), "samples": rss},
        "boundedMemoryAssertion": "state < 512 KiB and child RSS < 512 MiB per cycle",
    }


def qualify(runtime: Path, observations: int, modes: list[str], batch_size: int, soak_cycles: int, soak_mode: str) -> dict:
    if observations < 1 or observations > 4_096:
        raise ValueError("observations must be between 1 and 4096 per bounded phase")
    if batch_size < 1 or batch_size > 1_024:
        raise ValueError("batch-size must be between 1 and 1024")
    if soak_cycles < 0 or soak_cycles > 100:
        raise ValueError("soak-cycles must be between 0 and 100")
    phases = [_phase(runtime, mode, observations, batch_size) for mode in modes]
    decision_rates = [rate for phase in phases for rate in phase["decisionSecondsPerObservation"]]
    feedback_rates = [rate for phase in phases for rate in phase["feedbackSecondsPerObservation"]]
    result = {
        "schemaVersion": SCHEMA_VERSION,
        "status": "PASS",
        "mode": "synthetic-consumer-qualification",
        "evidenceType": "simulated",
        "consumerQualification": "simulated-only",
        "phases": phases,
        "benchmark": {
            "decisionSecondsPerObservationP50": _percentile(decision_rates, 0.50),
            "decisionSecondsPerObservationP95": _percentile(decision_rates, 0.95),
            "decisionSecondsPerObservationP99": _percentile(decision_rates, 0.99),
            "feedbackSecondsPerObservationP50": _percentile(feedback_rates, 0.50),
            "feedbackSecondsPerObservationP95": _percentile(feedback_rates, 0.95),
            "feedbackSecondsPerObservationP99": _percentile(feedback_rates, 0.99),
            "observationsPerPhase": observations,
            "batchSize": batch_size,
        },
        "resourceCapProbe": _resource_cap_probe(runtime),
        "evidence": [
            "real-child-process",
            "preview-handshake",
            "restart-persistence",
            "decision-outcome-feedback",
            "consumer-host-owned-action-boundary",
            "structured-inspection",
            "snapshot-latency",
            "resource-cap-rejection",
        ],
    }
    if soak_cycles:
        result["soak"] = _bounded_soak(runtime, soak_mode, soak_cycles, observations, batch_size)
        if result["soak"]["status"] != "PASS" or result["resourceCapProbe"]["status"] != "PASS":
            result["status"] = "FAIL"
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--observations", type=int, default=1000)
    parser.add_argument("--batch-size", type=int, default=DEFAULT_BATCH_SIZE)
    parser.add_argument("--soak-cycles", type=int, default=0)
    parser.add_argument("--soak-mode", choices=("simulated-consumer", "pm-style", "network-style"), default="simulated-consumer")
    parser.add_argument("--mode", action="append", choices=("simulated-consumer", "pm-style", "network-style"), dest="modes")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    modes = args.modes or ["simulated-consumer", "pm-style", "network-style"]
    try:
        result = qualify(args.runtime, args.observations, modes, args.batch_size, args.soak_cycles, args.soak_mode)
    except (OSError, ValueError, RuntimeError, json.JSONDecodeError, subprocess.SubprocessError) as error:
        result = {"schemaVersion": SCHEMA_VERSION, "status": "FAIL", "mode": "synthetic-consumer-qualification", "evidenceType": "simulated", "consumerQualification": "simulated-only", "error": str(error)}
    print(json.dumps(result, ensure_ascii=False, sort_keys=True, separators=(",", ":") if args.json else None))
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
