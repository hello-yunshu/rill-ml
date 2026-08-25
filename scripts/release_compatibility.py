#!/usr/bin/env python3
"""Read SemVer compatibility baselines from release-plan.toml."""

from __future__ import annotations

import argparse
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - CI uses Python 3.11+
    try:
        import tomli as tomllib
    except ModuleNotFoundError:
        tomllib = None


def load(root: Path) -> tuple[str, list[str]]:
    if tomllib is None:
        raise SystemExit("Python 3.11+ or the tomli package is required to read release-plan.toml")
    plan = tomllib.loads((root / "release-plan.toml").read_text(encoding="utf-8"))
    compatibility = plan.get("compatibility", {})
    authoritative = compatibility.get("authoritative")
    historical = compatibility.get("historical", [])
    if not isinstance(authoritative, str) or not authoritative:
        raise SystemExit("release-plan.toml [compatibility].authoritative is required")
    if not isinstance(historical, list) or not all(isinstance(item, str) for item in historical):
        raise SystemExit("release-plan.toml [compatibility].historical must be a list of strings")
    if authoritative in historical:
        raise SystemExit("authoritative baseline must not also be historical")
    return authoritative, historical


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--authoritative", action="store_true")
    parser.add_argument("--historical", action="store_true")
    args = parser.parse_args()
    if args.authoritative == args.historical:
        parser.error("choose exactly one of --authoritative or --historical")
    root = Path(__file__).resolve().parents[1]
    authoritative, historical = load(root)
    if args.authoritative:
        print(authoritative)
    else:
        print(" ".join(historical))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
