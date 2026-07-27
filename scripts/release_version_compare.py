#!/usr/bin/env python3
"""Compare two SemVer versions, including prerelease identifiers.

Used by Auto Release to ensure the candidate release version is strictly
newer than the latest existing tag.  SemVer 2.0 precedence rules are
implemented: a prerelease version has *lower* precedence than the same
major.minor.patch without a prerelease identifier, and prerelease
identifiers are compared field-by-field (numeric < alphanumeric, longer
prerelease wins when prefixes are equal).

Usage::

    python3 scripts/release_version_compare.py CURRENT PREVIOUS

Exit codes:
* 0 — CURRENT is strictly newer than PREVIOUS.
* 1 — CURRENT is not strictly newer than PREVIOUS (or either is invalid).
"""

from __future__ import annotations

import re
import sys

SEMVER_RE = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$"
)


def parse(version: str) -> tuple[int, int, int, list[str]]:
    match = SEMVER_RE.fullmatch(version)
    if not match:
        raise ValueError(f"not valid SemVer: {version!r}")
    major, minor, patch = int(match.group(1)), int(match.group(2)), int(match.group(3))
    pre = match.group(4)
    prerelease = pre.split(".") if pre else []
    return (major, minor, patch, prerelease)


def _compare_prerelease(a: list[str], b: list[str]) -> int:
    """Return -1/0/1 per SemVer 2.0 prerelease precedence."""
    # No prerelease = higher precedence than any prerelease.
    if not a and not b:
        return 0
    if not a:
        return 1
    if not b:
        return -1
    for x, y in zip(a, b):
        if x == y:
            continue
        x_is_num = x.isdigit()
        y_is_num = y.isdigit()
        if x_is_num and y_is_num:
            xi, yi = int(x), int(y)
            return -1 if xi < yi else (1 if xi > yi else 0)
        if x_is_num and not y_is_num:
            return -1  # numeric < alphanumeric
        if y_is_num and not x_is_num:
            return 1
        return -1 if x < y else 1
    # All compared fields equal: longer prerelease wins.
    return -1 if len(a) < len(b) else (1 if len(a) > len(b) else 0)


def compare(current: str, previous: str) -> int:
    """Return -1/0/1 for current vs previous precedence."""
    c = parse(current)
    p = parse(previous)
    for i in range(3):
        if c[i] != p[i]:
            return -1 if c[i] < p[i] else 1
    return _compare_prerelease(c[3], p[3])


def main() -> int:
    if len(sys.argv) != 3:
        print(
            "usage: release_version_compare.py CURRENT PREVIOUS",
            file=sys.stderr,
        )
        return 2
    current, previous = sys.argv[1], sys.argv[2]
    try:
        result = compare(current, previous)
    except ValueError as error:
        print(f"release_version_compare: {error}", file=sys.stderr)
        return 1
    if result <= 0:
        print(
            f"release version {current} must be newer than {previous}",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
