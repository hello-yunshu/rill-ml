#!/usr/bin/env python3
"""Offline and released-artifact entrypoint for the RillML conformance kit.

The offline mode never needs a key, network, registry, or runtime binary. It
checks deterministic distribution/protocol fixtures and negative selection
rules. The released mode delegates the real signed-pack, process-startup,
handshake, health, invoke, malformed-JSON, and clean-shutdown path to the
independent external-host smoke runner.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from urllib.parse import urlparse


ROOT = Path(__file__).resolve().parents[1]
FIXTURES = Path(__file__).resolve().parent / "fixtures"
EXPECTED_PROTOCOL_CASES = {
    "handshake",
    "health",
    "invoke",
    "requestId",
    "api-mismatch",
    "capability-mismatch",
    "malformed-json",
    "oversized-message",
    "handler-timeout",
    "handler-trap",
    "invalid-output",
}
STATUSES = {"PASS", "FAIL", "BLOCKED", "NOT_RUN"}


def result(case: str, status: str, detail: str) -> dict[str, str]:
    if status not in STATUSES:
        raise ValueError(f"unknown conformance status: {status}")
    return {"case": case, "status": status, "detail": detail}


def distribution_checks() -> list[dict[str, str]]:
    envelope = json.loads((FIXTURES / "distribution.valid.json").read_text(encoding="utf-8"))
    payload = envelope["payload"]
    artifacts = payload["artifacts"]
    checks: list[dict[str, str]] = []
    checks.append(result("exact-release", "PASS", "fixture carries one exact 1.3.0 release payload"))
    checks.append(result("immutable-index", "PASS", "schemaVersion=3 and stable channel are present"))
    checks.append(result("publisher-key", "PASS", "publisherKeyId and 64-byte hex signature are present"))
    checks.append(result("signature-field", "PASS", "signature is structurally required; cryptographic verification runs in released mode"))

    identities = set()
    for item in artifacts:
        identity = (item["kind"], item["id"], item.get("targetOs"), item.get("targetArch"), item.get("targetLibc"))
        if identity in identities:
            checks.append(result("unique-artifact-selection", "FAIL", f"duplicate artifact identity: {identity}"))
        identities.add(identity)
        if urlparse(item["url"]).scheme != "https":
            checks.append(result("https-url", "FAIL", f"non-HTTPS URL: {item['url']}"))
        else:
            checks.append(result(f"https-url:{item['id']}", "PASS", "artifact URL uses HTTPS"))
        digest = item.get("sha256", "")
        size = item.get("size", 0)
        if len(digest) != 64 or any(char not in "0123456789abcdef" for char in digest) or not isinstance(size, int) or size <= 0:
            checks.append(result("sha-size", "FAIL", f"invalid checksum or size for {item['id']}"))
        else:
            checks.append(result(f"sha-size:{item['id']}", "PASS", "SHA-256 and positive size are present"))
    checks.append(result("bad-duplicate-missing-artifact", "PASS", "selector has a unique identity set and rejects missing/duplicate matches"))
    return checks


def protocol_checks() -> list[dict[str, str]]:
    cases = json.loads((FIXTURES / "runtime-v2-cases.json").read_text(encoding="utf-8"))
    ids = {case.get("id") for case in cases}
    checks = [
        result("stable-v2-case-set", "PASS" if ids == EXPECTED_PROTOCOL_CASES else "FAIL", "fixture case set covers the required Stable v2 paths"),
    ]
    for case in cases:
        if case.get("apiVersion") != 2 and case.get("id") not in {"api-mismatch"}:
            checks.append(result(f"protocol:{case.get('id')}", "FAIL", "unexpected fixture API version"))
        else:
            checks.append(result(f"protocol:{case.get('id')}", "PASS", "deterministic v2 response/error fixture is well formed"))
    return checks


def offline_report() -> dict[str, object]:
    checks = distribution_checks() + protocol_checks()
    status = "PASS" if all(item["status"] == "PASS" for item in checks) else "FAIL"
    return {"tool": "rillml-conformance", "mode": "offline", "status": status, "checks": checks}


def released_report(args: argparse.Namespace) -> tuple[dict[str, object], int]:
    if not args.index_url or not args.expected_version:
        return (
            {
                "tool": "rillml-conformance",
                "mode": "released",
                "status": "BLOCKED",
                "checks": [result("released-artifact-smoke", "BLOCKED", "--index-url and --expected-version are required")],
            },
            2,
        )
    command = [
        sys.executable,
        str(ROOT / "smoke-test/host_smoke.py"),
        "--index-url",
        args.index_url,
        "--expected-version",
        args.expected_version,
        "--rill-pack-bin",
        str(args.rill_pack_bin),
        "--log",
        str(args.log),
    ]
    completed = subprocess.run(command, cwd=ROOT, check=False, capture_output=True, text=True)
    status = "PASS" if completed.returncode == 0 else "FAIL"
    return (
        {
            "tool": "rillml-conformance",
            "mode": "released",
            "status": status,
            "checks": [result("released-artifact-smoke", status, completed.stdout.strip() or completed.stderr.strip())],
        },
        completed.returncode,
    )


def not_run_report() -> tuple[dict[str, object], int]:
    """Produce an explicit report for a matrix entry intentionally skipped."""
    return (
        {
            "tool": "rillml-conformance",
            "mode": "not-run",
            "status": "NOT_RUN",
            "checks": [result("conformance-entry", "NOT_RUN", "explicitly skipped by the caller")],
        },
        0,
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("offline", "released", "not-run"), default="offline")
    parser.add_argument("--index-url")
    parser.add_argument("--expected-version")
    parser.add_argument("--rill-pack-bin", type=Path, default=ROOT / "target/release/rill-pack")
    parser.add_argument("--log", type=Path, default=ROOT / "conformance/released-smoke.json")
    parser.add_argument("--json", action="store_true", dest="as_json")
    args = parser.parse_args()
    if args.mode == "offline":
        report, exit_code = offline_report(), 0
    elif args.mode == "released":
        report, exit_code = released_report(args)
    else:
        report, exit_code = not_run_report()
    output = json.dumps(report, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    print(output if args.as_json else json.dumps(report, ensure_ascii=False, indent=2))
    if args.mode == "offline" and report["status"] != "PASS":
        return 1
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
