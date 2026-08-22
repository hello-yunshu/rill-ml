#!/usr/bin/env python3
"""Validate a consumer-owned RillML adoption record without network access."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from urllib.parse import urlparse


SCHEMA_VERSION = 1
REQUIRED = {
    "schemaVersion",
    "consumer",
    "upstreamRepository",
    "testedRelease",
    "supportedRelease",
    "pinnedRelease",
    "protocolApi",
    "protocolStatus",
    "handlerApi",
    "platform",
    "qualification",
    "evidenceCommit",
    "evidenceRun",
}
STATUSES = {"tested", "supported", "not-tested", "blocked"}
QUALIFICATIONS = {"example/non-authoritative", "tested", "supported", "blocked", "not-qualified"}


def validate(value: object) -> list[str]:
    errors: list[str] = []
    if not isinstance(value, dict):
        return ["record must be a JSON object"]
    keys = set(value)
    missing = sorted(REQUIRED - keys)
    unknown = sorted(keys - REQUIRED)
    if missing:
        errors.append("missing required fields: " + ", ".join(missing))
    if unknown:
        errors.append("unknown fields: " + ", ".join(unknown))
    if value.get("schemaVersion") != SCHEMA_VERSION:
        errors.append("unsupported schemaVersion (only 1 is accepted)")

    def string_field(name: str, max_length: int = 256) -> None:
        item = value.get(name)
        if not isinstance(item, str) or not item or len(item) > max_length:
            errors.append(f"{name} must be a non-empty string of at most {max_length} characters")

    for name in ("consumer", "testedRelease", "supportedRelease", "protocolApi", "handlerApi"):
        string_field(name, 96 if name == "consumer" else 48)
    upstream = value.get("upstreamRepository")
    parsed = urlparse(upstream) if isinstance(upstream, str) else None
    if parsed is None or parsed.scheme != "https" or not parsed.netloc or any(char.isspace() for char in upstream):
        errors.append("upstreamRepository must be an HTTPS URL")
    pinned = value.get("pinnedRelease")
    if pinned is not None and (not isinstance(pinned, str) or not pinned or len(pinned) > 48):
        errors.append("pinnedRelease must be null or a non-empty short string")
    if not isinstance(value.get("protocolApi"), str) or not re.fullmatch(r"v[0-9]+", value.get("protocolApi", "")):
        errors.append("protocolApi must look like vN")
    if not isinstance(value.get("handlerApi"), str) or not re.fullmatch(r"v[0-9]+", value.get("handlerApi", "")):
        errors.append("handlerApi must look like vN")
    if value.get("protocolStatus") not in STATUSES:
        errors.append("protocolStatus is not a recognized status")
    if value.get("qualification") not in QUALIFICATIONS:
        errors.append("qualification is not a recognized status")

    platform = value.get("platform")
    if not isinstance(platform, dict) or set(platform) - {"os", "arch", "libc"} or "os" not in platform or "arch" not in platform:
        errors.append("platform must contain only os, arch, and optional libc")
    elif any(not isinstance(platform.get(name), str) or not platform[name] for name in ("os", "arch")):
        errors.append("platform.os and platform.arch must be non-empty strings")
    elif "libc" in platform and (not isinstance(platform["libc"], str) or not platform["libc"]):
        errors.append("platform.libc must be a non-empty string when present")

    commit = value.get("evidenceCommit")
    if not isinstance(commit, str) or not re.fullmatch(r"[0-9a-f]{7,64}", commit):
        errors.append("evidenceCommit must be a lowercase hexadecimal commit id")
    run = value.get("evidenceRun")
    if run is not None and (not isinstance(run, str) or not run or len(run) > 256):
        errors.append("evidenceRun must be null or a non-empty short string")
    return sorted(set(errors))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("record", type=Path)
    parser.add_argument("--json", action="store_true", dest="as_json")
    args = parser.parse_args()
    try:
        value = json.loads(args.record.read_text(encoding="utf-8"))
        errors = validate(value)
    except (OSError, json.JSONDecodeError) as error:
        errors = [f"cannot read JSON: {error}"]
    result = {"status": "PASS" if not errors else "FAIL", "errors": errors, "path": str(args.record)}
    if args.as_json:
        print(json.dumps(result, ensure_ascii=False, sort_keys=True, separators=(",", ":")))
    else:
        print(result["status"])
        for error in errors:
            print(f"- {error}")
    return 0 if not errors else 1


if __name__ == "__main__":
    raise SystemExit(main())
