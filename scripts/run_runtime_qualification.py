#!/usr/bin/env python3
"""Run a bounded real-process Preview v3 qualification smoke.

This is intentionally small and deterministic. It is a harness implementation
for later 1.5 simulations, not a synthetic qualification claim by itself.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import tempfile
from pathlib import Path


SCHEMA_VERSION = 1
HASH = "ab" * 32


def envelope(request_id: str, capability: str | None, generation: int, request: dict) -> dict:
    value = {
        "requestId": request_id,
        "apiVersion": 3,
        "clientIdentity": {"name": "rill-qualification", "version": "1"},
        "modelGeneration": 0,
        "stateGeneration": generation,
        "payloadLimit": 1024 * 1024,
        "request": request,
    }
    if capability is not None:
        value["capability"] = capability
        value["featureSchemaHash"] = HASH
    return value


def run_process(runtime: Path, state: Path, requests: list[dict]) -> list[dict]:
    payload = b"".join(json.dumps(item, separators=(",", ":")).encode() + b"\n" for item in requests)
    result = subprocess.run(
        [str(runtime), "preview-serve", "--state", str(state)],
        input=payload,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.decode(errors="replace"))
    return [json.loads(line) for line in result.stdout.splitlines() if line.strip()]


def qualify(runtime: Path, observations: int) -> dict:
    if observations < 1 or observations > 1000:
        raise ValueError("observations must be between 1 and 1000 for a bounded run")
    with tempfile.TemporaryDirectory(prefix="rill-qualification-") as directory:
        state = Path(directory) / "runtime-state.json"
        requests = [envelope("hello", None, 0, {"method": "handshake"})]
        requests += [
            envelope(
                f"decision-{index}",
                "org.rill.preview.decide",
                index,
                {"method": "decide", "context": {"features": [float(index)]}},
            )
            for index in range(observations)
        ]
        first = run_process(runtime, state, requests)
        if first[0]["response"].get("channel") != "preview":
            raise RuntimeError("preview handshake did not advertise channel=preview")
        decisions = [item for item in first[1:] if item["response"].get("kind") == "result"]
        if len(decisions) != observations:
            raise RuntimeError("not every decision was accepted")
        feedback = []
        generation = observations
        for index in range(observations):
            feedback.append(
                envelope(
                    f"feedback-{index}",
                    "org.rill.preview.feedback",
                    generation,
                    {
                        "method": "feedback",
                        "decisionId": f"decision-{index}",
                        "selectedArm": 0,
                        "reward": 1.0,
                        "outcomeTimeMs": index + 1,
                        "generation": 0,
                    },
                )
            )
            generation += 1
        second = run_process(runtime, state, feedback)
        if any(item["response"].get("kind") != "result" for item in second):
            raise RuntimeError("feedback after restart was not accepted")
        duplicate = run_process(
            runtime,
            state,
            [
                envelope(
                    "duplicate",
                    "org.rill.preview.feedback",
                    generation,
                    {
                        "method": "feedback",
                        "decisionId": "decision-0",
                        "selectedArm": 0,
                        "reward": 1.0,
                        "outcomeTimeMs": 1,
                        "generation": 0,
                    },
                )
            ],
        )
        if duplicate[0]["response"].get("error", {}).get("code") != "duplicateFeedback":
            raise RuntimeError("duplicate feedback was not rejected")
    return {
        "schemaVersion": SCHEMA_VERSION,
        "scenarioId": "preview-v3-restart-ledger-smoke",
        "status": "PASS",
        "mode": "bounded-smoke",
        "observations": observations,
        "evidence": [
            "real-child-process",
            "preview-handshake",
            "atomic-state-restart",
            "delayed-feedback",
            "duplicate-feedback-rejection",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--observations", type=int, default=3)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    try:
        result = qualify(args.runtime, args.observations)
    except (OSError, ValueError, RuntimeError, json.JSONDecodeError) as error:
        result = {"schemaVersion": SCHEMA_VERSION, "status": "FAIL", "error": str(error)}
    print(json.dumps(result, ensure_ascii=False, sort_keys=True, separators=(",", ":") if args.json else None))
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
