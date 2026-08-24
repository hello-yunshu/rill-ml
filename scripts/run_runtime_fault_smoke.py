#!/usr/bin/env python3
"""Execute deterministic, bounded fault scenarios against a runtime binary.

This is a harness smoke, not a production platform claim. Extended chaos and
long-running fault injection belong to the post-push final qualification.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import tempfile
import time
from pathlib import Path


SCHEMA_VERSION = 1


def run_faults(runtime: Path, registry: Path) -> dict:
    document = json.loads(registry.read_text(encoding="utf-8"))
    scenarios = document.get("scenarios", [])
    results: list[dict] = []
    with tempfile.TemporaryDirectory(prefix="rill-fault-smoke-") as directory:
        root = Path(directory)
        corrupted = root / "corrupted.json"
        corrupted.write_text("{", encoding="utf-8")
        process = subprocess.run(
            [str(runtime), "preview-serve", "--state", str(corrupted)],
            input=b"",
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        results.append({"scenarioId": "corrupted-state", "status": "PASS" if process.returncode != 0 else "FAIL"})

        truncated = root / "truncated.json"
        truncated.write_bytes(b"{\"formatVersion\":1")
        process = subprocess.run(
            [str(runtime), "preview-serve", "--state", str(truncated)],
            input=b"",
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        results.append({"scenarioId": "truncated-state", "status": "PASS" if process.returncode != 0 else "FAIL"})

        state = root / "malformed-ipc.json"
        process = subprocess.run(
            [str(runtime), "preview-serve", "--state", str(state)],
            input=b"{\n",
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        malformed_pass = process.returncode == 0 and b"invalidJson" in process.stdout
        results.append({"scenarioId": "malformed-ipc", "status": "PASS" if malformed_pass else "FAIL"})

        concurrent_state = root / "concurrent-startup.json"
        first = subprocess.Popen(
            [str(runtime), "preview-serve", "--state", str(concurrent_state)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        try:
            time.sleep(0.2)
            second = subprocess.run(
                [str(runtime), "preview-serve", "--state", str(concurrent_state)],
                input=b"",
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            concurrent_pass = second.returncode != 0
        finally:
            first.terminate()
            try:
                first.communicate(timeout=2)
            except subprocess.TimeoutExpired:
                first.kill()
                first.communicate()
        results.append({"scenarioId": "concurrent-startup", "status": "PASS" if concurrent_pass else "FAIL"})

        stale_state = root / "stale-lock.json"
        stale_lock = Path(f"{stale_state}.lock")
        stale_lock.write_text("pid=999999\n", encoding="utf-8")
        process = subprocess.run(
            [str(runtime), "preview-serve", "--state", str(stale_state)],
            input=b"{\n",
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        stale_lock_pass = process.returncode == 0 and b"invalidJson" in process.stdout
        results.append({"scenarioId": "stale-lock", "status": "PASS" if stale_lock_pass else "FAIL"})

        readonly_dir = root / "readonly-state-dir"
        readonly_dir.mkdir()
        os.chmod(readonly_dir, 0o500)
        try:
            process = subprocess.run(
                [str(runtime), "preview-serve", "--state", str(readonly_dir / "state.json")],
                input=b"",
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            readonly_pass = process.returncode != 0
        finally:
            os.chmod(readonly_dir, 0o700)
        results.append({"scenarioId": "readonly-state-dir", "status": "PASS" if readonly_pass else "FAIL"})

    known = {item["id"] for item in scenarios}
    covered = {item["scenarioId"] for item in results}
    required = {"corrupted-state", "truncated-state", "malformed-ipc"}
    missing = sorted(required - known)
    unexecuted = sorted(known - covered)
    return {
        "schemaVersion": SCHEMA_VERSION,
        "status": "PASS" if all(item["status"] == "PASS" for item in results) and not missing else "FAIL",
        "mode": "deterministic-fault-smoke",
        "evidenceType": "simulated",
        "registryScenarioCount": len(scenarios),
        "coveredScenarioIds": sorted(covered),
        "executionCoverage": f"{len(covered)}/{len(known)}",
        "results": results,
        "missingRegistryIds": missing,
        "unexecutedRegistryIds": unexecuted,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--registry", type=Path, default=Path("schemas/runtime-fault-scenarios-v1.json"))
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    try:
        result = run_faults(args.runtime, args.registry)
    except (OSError, ValueError, RuntimeError, json.JSONDecodeError) as error:
        result = {"schemaVersion": SCHEMA_VERSION, "status": "FAIL", "evidenceType": "simulated", "error": str(error)}
    print(json.dumps(result, ensure_ascii=False, sort_keys=True, separators=(",", ":") if args.json else None))
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
