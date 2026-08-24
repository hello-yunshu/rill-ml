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


class _RuntimeSession:
    """Persistent newline-delimited Preview session with request correlation."""

    def __init__(self, runtime: Path, state: Path) -> None:
        self.process = subprocess.Popen(
            [str(runtime), "preview-serve", "--state", str(state)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )

    def request(self, request: dict) -> tuple[dict, float]:
        if self.process.stdin is None or self.process.stdout is None:
            raise RuntimeError("runtime session pipes are unavailable")
        started = time.perf_counter()
        self.process.stdin.write(json.dumps(request, separators=(",", ":")) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        elapsed = time.perf_counter() - started
        if not line:
            stderr = self.process.stderr.read() if self.process.stderr is not None else ""
            raise RuntimeError(f"runtime session ended before response: {stderr}")
        response = json.loads(line)
        if response.get("requestId") != request.get("requestId"):
            raise RuntimeError(
                f"response/request id mismatch: {response.get('requestId')} != {request.get('requestId')}"
            )
        return response, elapsed

    def close(self) -> None:
        if self.process.stdin is not None:
            self.process.stdin.close()
        return_code = self.process.wait(timeout=10)
        if return_code != 0:
            stderr = self.process.stderr.read() if self.process.stderr is not None else ""
            raise RuntimeError(f"runtime session exited with {return_code}: {stderr}")


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
        session = _RuntimeSession(runtime, state)
        cold_start, cold_elapsed = session.request(
            envelope(f"{mode}-cold-start", None, 0, {"method": "handshake"})
        )
        if cold_start["response"].get("channel") != "preview":
            raise RuntimeError(f"{mode}: preview channel was not advertised")

        decision_latencies: list[float] = []
        state_generation = 0
        accepted = 0
        for index in range(observations):
            request = _decision_requests(mode, index, 1, state_generation)[0]
            response, elapsed = session.request(request)
            if response["response"].get("kind") != "result":
                raise RuntimeError(f"{mode}: decision {index} was rejected")
            decision_latencies.append(elapsed)
            accepted += 1
            state_generation = response["stateGeneration"]
        session.close()

        state_bytes_after_decisions = state.stat().st_size
        session = _RuntimeSession(runtime, state)
        restore, restore_elapsed = session.request(
            envelope(f"{mode}-restore-inspect", "org.rill.preview.inspect", state_generation, {"method": "inspect"})
        )
        if restore["response"].get("kind") != "inspection":
            raise RuntimeError(f"{mode}: restart inspect was rejected")
        state_generation = restore["stateGeneration"]

        feedback_latencies: list[float] = []
        feedback_accepted = 0
        for index in range(observations):
            request = _feedback_requests(mode, index, 1, state_generation)[0]
            response, elapsed = session.request(request)
            if response["response"].get("kind") != "result":
                raise RuntimeError(f"{mode}: feedback {index} after restart was rejected")
            feedback_latencies.append(elapsed)
            feedback_accepted += 1
            state_generation = response["stateGeneration"]

        state_bytes_after_feedback = state.stat().st_size
        snapshot, snapshot_elapsed = session.request(
            envelope(f"{mode}-snapshot", "org.rill.preview.snapshot", state_generation, {"method": "snapshot"})
        )
        if snapshot["response"].get("kind") != "snapshot":
            raise RuntimeError(f"{mode}: snapshot was rejected")

        inspect, inspect_elapsed = session.request(
            envelope(f"{mode}-inspect", "org.rill.preview.inspect", state_generation, {"method": "inspect"})
        )
        session.close()
        summary = inspect["response"].get("summary", {})
        if summary.get("pendingDecisions") != 0 or summary.get("completedDecisions") != observations:
            raise RuntimeError(f"{mode}: ledger summary did not converge: {summary}")
        return {
            "status": "PASS",
            "mode": mode,
            "observations": observations,
            "batchSize": batch_size,
            "decisionsAccepted": accepted,
            "feedbackAccepted": feedback_accepted,
            "pendingDecisions": summary.get("pendingDecisions"),
            "completedDecisions": summary.get("completedDecisions"),
            "decisionLatencySeconds": decision_latencies,
            "feedbackLatencySeconds": feedback_latencies,
            "coldStartSeconds": cold_elapsed,
            "restoreSeconds": restore_elapsed,
            "snapshotSeconds": snapshot_elapsed,
            "inspectSeconds": inspect_elapsed,
            "stateBytesAfterDecisions": state_bytes_after_decisions,
            "stateBytesAfterFeedback": state_bytes_after_feedback,
            "stateGeneration": inspect.get("stateGeneration"),
            "processTreeChildHighWaterMarkBytes": _child_max_rss_bytes(),
            "rssEvidence": {
                "metric": "processTreeChildHighWaterMarkBytes",
                "sampleCount": 1,
                "evidencePlatform": sys.platform,
                "perPidPerCycle": False,
                "note": "resource.RUSAGE_CHILDREN high-water mark; not an independent per-cycle RSS sample",
            },
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
    rss = [sample["processTreeChildHighWaterMarkBytes"] for sample in samples]
    return {
        "status": "PASS" if max(sizes) < 512 * 1024 and max(rss) < 512 * 1024 * 1024 else "FAIL",
        "evidenceType": "simulated",
        "mode": mode,
        "cycles": cycles,
        "observationsPerCycle": observations,
        "totalObservations": cycles * observations,
        "stateBytes": {"min": min(sizes), "max": max(sizes), "samples": sizes},
        "processTreeChildHighWaterMarkBytes": {"min": min(rss), "max": max(rss), "samples": rss},
        "boundedMemoryAssertion": "state < 512 KiB and process-tree child high-water mark < 512 MiB per cycle",
    }


def _continuous_same_state_saturation(runtime: Path) -> dict:
    """Exercise one durable state through the completed-history boundary."""
    total = 4_096
    with tempfile.TemporaryDirectory(prefix="rill-continuous-ledger-") as directory:
        state = Path(directory) / "runtime-state.json"
        session = _RuntimeSession(runtime, state)
        handshake, _ = session.request(
            envelope("saturation-handshake", None, 0, {"method": "handshake"})
        )
        if handshake["response"].get("channel") != "preview":
            raise RuntimeError("continuous saturation did not enter Preview channel")
        generation = 0
        for index in range(total):
            decision, _ = session.request(
                _decision_requests("saturation", index, 1, generation)[0]
            )
            if decision["response"].get("kind") != "result":
                raise RuntimeError(f"decision {index} rejected before completed capacity")
            generation = decision["stateGeneration"]
            feedback, _ = session.request(
                _feedback_requests("saturation", index, 1, generation)[0]
            )
            if feedback["response"].get("kind") != "result":
                raise RuntimeError(f"feedback {index} rejected before completed capacity")
            generation = feedback["stateGeneration"]
        rejected, _ = session.request(
            _decision_requests("saturation", total, 1, generation)[0]
        )
        error = rejected["response"].get("error", {})
        rejected_generation = rejected.get("stateGeneration")
        inspect, _ = session.request(
            envelope("saturation-inspect", "org.rill.preview.inspect", generation, {"method": "inspect"})
        )
        session.close()
        summary = inspect["response"].get("summary", {})
        passed = (
            error.get("code") == "capacityExceeded"
            and rejected_generation == generation
            and summary.get("completedDecisions") == total
            and summary.get("pendingDecisions") == 0
        )
        session = _RuntimeSession(runtime, state)
        restored, _ = session.request(
            envelope("saturation-restart-inspect", "org.rill.preview.inspect", inspect["stateGeneration"], {"method": "inspect"})
        )
        session.close()
        restored_summary = restored["response"].get("summary", {})
        passed = passed and restored_summary.get("completedDecisions") == total
        return {
            "status": "PASS" if passed else "FAIL",
            "evidenceType": "simulated",
            "sameState": True,
            "completedBeforeRejection": summary.get("completedDecisions"),
            "rejectedRequestCode": error.get("code"),
            "generationUnchangedOnRejection": rejected_generation == generation,
            "completedAfterRestart": restored_summary.get("completedDecisions"),
        }


def qualify(runtime: Path, observations: int, modes: list[str], batch_size: int, soak_cycles: int, soak_mode: str) -> dict:
    if observations < 1 or observations > 4_096:
        raise ValueError("observations must be between 1 and 4096 per bounded phase")
    if batch_size < 1 or batch_size > 1_024:
        raise ValueError("batch-size must be between 1 and 1024")
    if soak_cycles < 0 or soak_cycles > 100:
        raise ValueError("soak-cycles must be between 0 and 100")
    phases = [_phase(runtime, mode, observations, batch_size) for mode in modes]
    decision_rates = [rate for phase in phases for rate in phase["decisionLatencySeconds"]]
    feedback_rates = [rate for phase in phases for rate in phase["feedbackLatencySeconds"]]
    resource_cap_probe = _resource_cap_probe(runtime)
    continuous_saturation = _continuous_same_state_saturation(runtime)
    result = {
        "schemaVersion": SCHEMA_VERSION,
        "status": "PASS",
        "mode": "synthetic-consumer-qualification",
        "evidenceType": "simulated",
        "consumerQualification": "simulated-only",
        "phases": phases,
        "benchmark": {
            "decisionLatencySecondsP50": _percentile(decision_rates, 0.50),
            "decisionLatencySecondsP95": _percentile(decision_rates, 0.95),
            "decisionLatencySecondsP99": _percentile(decision_rates, 0.99),
            "feedbackLatencySecondsP50": _percentile(feedback_rates, 0.50),
            "feedbackLatencySecondsP95": _percentile(feedback_rates, 0.95),
            "feedbackLatencySecondsP99": _percentile(feedback_rates, 0.99),
            "decisionLatencySecondsMax": max(decision_rates),
            "feedbackLatencySecondsMax": max(feedback_rates),
            "decisionLatencySampleCount": len(decision_rates),
            "feedbackLatencySampleCount": len(feedback_rates),
            "observationsPerPhase": observations,
            "batchSize": batch_size,
        },
        "resourceCapProbe": resource_cap_probe,
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
    required_checks = [
        {"checkId": f"consumer-phase:{phase['mode']}", "status": phase["status"]}
        for phase in phases
    ]
    required_checks.append(
        {"checkId": "resource-cap-probe", "status": resource_cap_probe["status"]}
    )
    required_checks.append(
        {"checkId": "continuous-same-state-saturation", "status": continuous_saturation["status"]}
    )
    result["continuousSameStateSaturation"] = continuous_saturation
    if soak_cycles:
        result["soak"] = _bounded_soak(runtime, soak_mode, soak_cycles, observations, batch_size)
        required_checks.append(
            {"checkId": "bounded-soak", "status": result["soak"]["status"]}
        )
    result["requiredChecks"] = required_checks
    result["status"] = "PASS" if all(item["status"] == "PASS" for item in required_checks) else "FAIL"
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
