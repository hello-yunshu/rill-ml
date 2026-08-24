#!/usr/bin/env python3
"""Require readable refs and reject fixed commit SHAs for GitHub Actions."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


SHA_RE = re.compile(r"^[0-9a-fA-F]{40}$")
USES_RE = re.compile(r"^\s*(?:-\s*)?uses:\s*([^\s#]+)")


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
            if reference.startswith("./"):
                references.append({"action": reference, "ref": "local", "path": str(path), "line": str(line_number)})
                continue
            if "@" not in reference:
                errors.append(f"{path}:{line_number}: missing @ref: {reference}")
                continue
            action, ref = reference.rsplit("@", 1)
            references.append({"action": action, "ref": ref, "path": str(path), "line": str(line_number)})
            if SHA_RE.fullmatch(ref):
                errors.append(f"{path}:{line_number}: fixed action SHA is forbidden: {reference}")
            elif not ref or ref.startswith("$"):
                errors.append(f"{path}:{line_number}: action ref must be a readable name: {reference}")
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
        print(f"{result['status']} action-refs ({result['referenceCount']} references)")
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
