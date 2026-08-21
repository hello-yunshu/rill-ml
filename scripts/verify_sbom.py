#!/usr/bin/env python3
"""Verify that both published SBOM formats carry the same release identity."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


def properties(items: object) -> dict[str, str]:
    if not isinstance(items, list):
        return {}
    return {
        item["name"]: item["value"]
        for item in items
        if isinstance(item, dict) and isinstance(item.get("name"), str) and isinstance(item.get("value"), str)
    }


def identity(value: object, version: str, tag: str, commit: str) -> list[str]:
    errors: list[str] = []
    if not isinstance(value, dict):
        return ["release identity must be an object"]
    if value.get("version") != version or value.get("tag") != tag or value.get("commit") != commit:
        errors.append("release identity does not match expected version, tag, and commit")
    artifacts = value.get("artifacts")
    if not isinstance(artifacts, list) or not artifacts:
        errors.append("release identity must include at least one artifact")
    else:
        for item in artifacts:
            if not isinstance(item, dict) or not isinstance(item.get("name"), str):
                errors.append("each artifact identity must include a name")
                continue
            if not isinstance(item.get("sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", item["sha256"]):
                errors.append(f"invalid artifact SHA-256 for {item['name']}")
            if not isinstance(item.get("size"), int) or item["size"] <= 0:
                errors.append(f"invalid artifact size for {item['name']}")
    return errors


def verify_cyclonedx(path: Path, version: str, tag: str, commit: str) -> list[str]:
    value = json.loads(path.read_text(encoding="utf-8"))
    errors: list[str] = []
    if value.get("bomFormat") != "CycloneDX" or value.get("specVersion") != "1.5":
        errors.append("CycloneDX format is not the declared version")
    component = value.get("metadata", {}).get("component", {})
    if component.get("version") != version:
        errors.append("CycloneDX metadata version mismatch")
    metadata_props = properties(value.get("metadata", {}).get("properties"))
    if metadata_props.get("rillml.release.tag") != tag or metadata_props.get("rillml.release.commit") != commit:
        errors.append("CycloneDX metadata release identity mismatch")
    release_props = properties(value.get("properties"))
    try:
        errors.extend(identity(json.loads(release_props["rillml.release.identity"]), version, tag, commit))
    except (KeyError, json.JSONDecodeError, TypeError):
        errors.append("CycloneDX release identity property is missing or invalid")
    return errors


def verify_spdx(path: Path, version: str, tag: str, commit: str) -> list[str]:
    value = json.loads(path.read_text(encoding="utf-8"))
    errors: list[str] = []
    if value.get("spdxVersion") != "SPDX-2.3" or value.get("name") != f"rill-ml-{version}":
        errors.append("SPDX format or document name mismatch")
    if value.get("documentNamespace") != f"https://rillml.dev/sbom/{tag}/{commit}":
        errors.append("SPDX document namespace mismatch")
    annotations = value.get("annotations", [])
    comments = [item.get("comment") for item in annotations if isinstance(item, dict)]
    try:
        raw_identity = next(comment for comment in comments if isinstance(comment, str) and comment.startswith("{"))
        errors.extend(identity(json.loads(raw_identity), version, tag, commit))
    except (StopIteration, json.JSONDecodeError, TypeError):
        errors.append("SPDX release identity annotation is missing or invalid")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--cdx", type=Path, required=True)
    parser.add_argument("--spdx", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--commit", required=True)
    args = parser.parse_args()
    errors = []
    try:
        errors.extend(verify_cyclonedx(args.cdx, args.version, args.tag, args.commit))
        errors.extend(verify_spdx(args.spdx, args.version, args.tag, args.commit))
    except (OSError, json.JSONDecodeError) as error:
        errors.append(f"cannot read SBOM: {error}")
    for error in sorted(set(errors)):
        print(f"FAIL {error}")
    print("PASS sbom-identity" if not errors else "FAIL sbom-identity")
    return 0 if not errors else 1


if __name__ == "__main__":
    raise SystemExit(main())
