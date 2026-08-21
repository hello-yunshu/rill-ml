#!/usr/bin/env python3
"""Fail closed when a GitHub Actions workflow uses a mutable ref."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


SHA_RE = re.compile(r"^[0-9a-f]{40}$")
USES_RE = re.compile(r"^\s*(?:-\s*)?uses:\s*([^\s#]+)")

# These are the exact commits reviewed for the 1.3.0 release gate. Keeping the
# allow-list local makes the check deterministic and avoids trusting a tag at
# validation time.
REVIEWED_SHAS = {
    "actions/checkout": {"3d3c42e5aac5ba805825da76410c181273ba90b1"},
    "actions/cache": {"55cc8345863c7cc4c66a329aec7e433d2d1c52a9"},
    "actions/download-artifact": {"3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c"},
    "actions/setup-python": {"ece7cb06caefa5fff74198d8649806c4678c61a1"},
    "actions/upload-artifact": {"043fb46d1a93c77aae656e7c1c64a875d1fc6a0a"},
    "docker/setup-buildx-action": {"37fe631027851001ddb9b187196cc803df7f5f0e"},
    "docker/setup-qemu-action": {"96fe6ef7f33517b61c61be40b68a1882f3264fb8"},
    "dtolnay/rust-toolchain": {
        "4360b52568e2003a75bf9bc1d59f33a8e3fc893c",
        "6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772",
        "7c8d7d138f5c09cef361f8214cf96882cd029cdb",
    },
    "github/codeql-action/init": {"db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28"},
    "github/codeql-action/analyze": {"db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28"},
    "rustsec/audit-check": {"858dc40f52ca2b8570b7a997c1c4e35c6fc9a432"},
    "Swatinem/rust-cache": {"6323deb102c322ba6fcbdcafc7e3dddab59af2b6"},
    "vmactions/freebsd-vm": {"83b151f58c6047089f4c80eb5ba2039d158ce093"},
    "xyzzylabs/setup-zig": {"df7066a4910fe13f4643390dbbd8ce6a785fff63"},
}


def scan(root: Path) -> dict[str, object]:
    errors: list[str] = []
    references: list[dict[str, str]] = []
    workflow_dir = root / ".github" / "workflows"
    for path in sorted(workflow_dir.glob("*.yml")) + sorted(workflow_dir.glob("*.yaml")):
        for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            match = USES_RE.match(line)
            if match is None:
                continue
            reference = match.group(1)
            if "@" not in reference:
                errors.append(f"{path}:{line_number}: missing @SHA: {reference}")
                continue
            action, sha = reference.rsplit("@", 1)
            references.append({"action": action, "sha": sha, "path": str(path), "line": str(line_number)})
            if not SHA_RE.fullmatch(sha):
                errors.append(f"{path}:{line_number}: mutable action ref: {reference}")
            elif action not in REVIEWED_SHAS:
                errors.append(f"{path}:{line_number}: action is not in reviewed allow-list: {action}")
            elif sha not in REVIEWED_SHAS[action]:
                errors.append(
                    f"{path}:{line_number}: unexpected SHA for {action}: {sha}"
                )
    if not references:
        errors.append("no workflow action references found")
    return {
        "status": "PASS" if not errors else "FAIL",
        "workflowCount": len(list(workflow_dir.glob("*.yml"))) + len(list(workflow_dir.glob("*.yaml"))),
        "referenceCount": len(references),
        "errors": sorted(set(errors)),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--json", action="store_true", dest="as_json")
    args = parser.parse_args()
    result = scan(args.root.resolve())
    if args.as_json:
        print(json.dumps(result, ensure_ascii=False, sort_keys=True, separators=(",", ":")))
    else:
        for error in result["errors"]:
            print(f"FAIL {error}")
        print(f"{result['status']} action-pins ({result['referenceCount']} references)")
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
