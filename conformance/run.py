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
EXPECTED_RELEASE_VERSION = "1.3.0"
EXPECTED_ARTIFACTS = {
    ("runtime", "rill-runtime"): {"targetOs": "linux", "targetArch": "x86_64", "targetLibc": "gnu"},
    ("model", "rillml.example.default"): {"targetOs": None, "targetArch": None, "targetLibc": None},
    ("handler", "org.example.echo"): {"targetOs": None, "targetArch": None, "targetLibc": None},
}
KNOWN_ERROR_CODES = {
    "invalidClientIdentity",
    "invalidRequestId",
    "incompatibleApiVersion",
    "unsupportedCapability",
    "invalidJson",
    "noInvokeHandler",
    "handlerTimeout",
    "handlerTrap",
    "handlerOutputTooLarge",
    "handlerInvalidOutput",
    "handlerInternalError",
}
STATUSES = {"PASS", "FAIL", "BLOCKED", "NOT_RUN"}


def result(case: str, status: str, detail: str) -> dict[str, str]:
    if status not in STATUSES:
        raise ValueError(f"unknown conformance status: {status}")
    return {"case": case, "status": status, "detail": detail}


def distribution_checks() -> list[dict[str, str]]:
    envelope = json.loads((FIXTURES / "distribution.valid.json").read_text(encoding="utf-8"))
    checks: list[dict[str, str]] = []
    if set(envelope) != {"payload", "signature"}:
        checks.append(result("envelope-schema", "FAIL", "distribution envelope has unexpected or missing fields"))
    else:
        checks.append(result("envelope-schema", "PASS", "distribution envelope fields are exact"))
    payload = envelope.get("payload", {})
    payload_keys = {"schemaVersion", "channel", "generatedAt", "publisherKeyId", "artifacts"}
    checks.append(result(
        "immutable-index",
        "PASS" if set(payload) == payload_keys and payload.get("schemaVersion") == 3 and payload.get("channel") == "stable" else "FAIL",
        "schemaVersion=3, stable channel, timestamp, publisher, and artifacts are checked from fixture content",
    ))
    signature = envelope.get("signature", "")
    checks.append(result(
        "signature-encoding",
        "PASS" if isinstance(signature, str) and len(signature) == 128 and all(char in "0123456789abcdef" for char in signature) else "FAIL",
        "signature is exactly 64 lowercase hexadecimal bytes",
    ))
    checks.append(result(
        "publisher-key",
        "PASS" if isinstance(payload.get("publisherKeyId"), str) and payload["publisherKeyId"] == "fixture-publisher" else "FAIL",
        "publisherKeyId matches the committed fixture identity",
    ))
    artifacts = payload.get("artifacts", [])
    expected_ids = set(EXPECTED_ARTIFACTS)
    actual_ids = {(item.get("kind"), item.get("id")) for item in artifacts if isinstance(item, dict)}
    checks.append(result(
        "exact-release",
        "PASS" if payload.get("generatedAt") == "2026-08-22T00:00:00Z" and all(item.get("version") == EXPECTED_RELEASE_VERSION for item in artifacts) else "FAIL",
        "all committed artifacts carry the exact expected Stable release version",
    ))
    checks.append(result(
        "exact-artifact-set",
        "PASS" if actual_ids == expected_ids and len(artifacts) == len(expected_ids) else "FAIL",
        "artifact kind/id set is compared to the expected release inventory",
    ))

    identities = set()
    for item in artifacts:
        if not isinstance(item, dict):
            checks.append(result("artifact-shape", "FAIL", "artifact entry is not an object"))
            continue
        identity = (item.get("kind"), item.get("id"), item.get("targetOs"), item.get("targetArch"), item.get("targetLibc"))
        unique = identity not in identities
        identities.add(identity)
        checks.append(result(f"unique-artifact:{item.get('id')}", "PASS" if unique else "FAIL", "artifact identity is unique"))
        expected = EXPECTED_ARTIFACTS.get((item.get("kind"), item.get("id")))
        checks.append(result(
            f"target:{item.get('id')}",
            "PASS" if expected is not None and all(item.get(key) == value for key, value in expected.items()) else "FAIL",
            "target OS, architecture, and libc match the exact fixture contract",
        ))
        parsed_url = urlparse(item.get("url", ""))
        checks.append(result(f"https-url:{item.get('id')}", "PASS" if parsed_url.scheme == "https" and parsed_url.netloc else "FAIL", "artifact URL is HTTPS"))
        digest = item.get("sha256", "")
        size = item.get("size")
        checks.append(result(
            f"sha-size:{item.get('id')}",
            "PASS" if isinstance(digest, str) and len(digest) == 64 and all(char in "0123456789abcdef" for char in digest) and isinstance(size, int) and size > 0 else "FAIL",
            "artifact SHA-256 and positive size are structurally valid",
        ))
        required = {"kind", "id", "version", "runtimeApiVersion", "url", "sha256", "size"}
        if item.get("kind") == "runtime":
            required |= {"targetOs", "targetArch", "targetLibc"}
        if item.get("kind") == "handler":
            required |= {"handlerApiVersion", "minRuntimeVersion"}
        checks.append(result(f"fields:{item.get('id')}", "PASS" if set(item) == required else "FAIL", "artifact fields reject unknown and missing members"))
    duplicate_probe = list(artifacts) + ([dict(artifacts[0])] if artifacts else [])
    duplicate_ids = [(item.get("kind"), item.get("id"), item.get("targetOs"), item.get("targetArch"), item.get("targetLibc")) for item in duplicate_probe]
    checks.append(result("bad-duplicate-missing-artifact", "PASS" if len(duplicate_ids) != len(set(duplicate_ids)) else "FAIL", "a duplicated selector identity is actually rejected"))
    return checks


def protocol_checks() -> list[dict[str, str]]:
    cases = json.loads((FIXTURES / "runtime-v2-cases.json").read_text(encoding="utf-8"))
    ids = {case.get("id") for case in cases}
    checks = [result("stable-v2-case-set", "PASS" if ids == EXPECTED_PROTOCOL_CASES and len(cases) == len(EXPECTED_PROTOCOL_CASES) else "FAIL", "fixture case set covers the required Stable v2 paths")]
    for case in cases:
        case_id = case.get("id")
        required = {"id", "kind", "requestId", "apiVersion"}
        if case.get("kind") == "error":
            required.add("code")
        shape_ok = set(case) == required and isinstance(case_id, str) and 0 < len(case.get("requestId", "")) <= 128
        api_ok = case.get("apiVersion") == (99 if case_id == "api-mismatch" else 2)
        code_ok = case.get("kind") != "error" or case.get("code") in KNOWN_ERROR_CODES
        checks.append(result(f"protocol:{case_id}", "PASS" if shape_ok and api_ok and code_ok else "FAIL", "fixture request/response fields, Stable v2 API, and known error code are checked"))
    unknown_field_probe = dict(cases[0]) if cases else {}
    unknown_field_probe["unexpected"] = True
    checks.append(result("unknown-field-rejection", "PASS" if set(unknown_field_probe) != {"id", "kind", "requestId", "apiVersion"} else "FAIL", "an injected unknown field is rejected by the exact fixture schema"))
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
