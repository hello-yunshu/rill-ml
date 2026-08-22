#!/usr/bin/env python3
"""Execute deterministic, bounded fault scenarios against a runtime binary.

This is a harness smoke, not a production platform claim. Extended chaos and
long-running fault injection belong to the post-push final qualification.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import tempfile
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

    known = {item["id"] for item in scenarios}
    covered = {item["scenarioId"] for item in results}
    required = {"corrupted-state", "truncated-state", "malformed-ipc"}
    missing = sorted(required - known)
    return {
        "schemaVersion": SCHEMA_VERSION,
        "status": "PASS" if all(item["status"] == "PASS" for item in results) and not missing else "FAIL",
        "mode": "deterministic-fault-smoke",
        "evidenceType": "simulated",
        "registryScenarioCount": len(scenarios),
        "coveredScenarioIds": sorted(covered),
        "results": results,
        "missingRegistryIds": missing,
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
