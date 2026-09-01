#!/usr/bin/env python3
"""Generate or verify the 1.0 Stable-crate public API baseline.

The baseline files in ``api-baseline/`` are the frozen public API surface
for the four Stable crates at the ``1.0.0-rc.1`` freeze point.  After RC,
additive additions are allowed but breaking changes must fail CI.

Usage::

    # Regenerate the baseline files (run once at freeze time, or when the
    # baseline is intentionally updated after review).
    python3 scripts/generate_api_baseline.py --generate

    # Verify the current public API still matches the frozen baseline.
    # Exits non-zero on any difference.
    python3 scripts/generate_api_baseline.py --verify

Requirements:
    * ``cargo-public-api`` v0.52 or later (``cargo install cargo-public-api --locked``).
    * The ``nightly`` Rust toolchain (``rustup toolchain install nightly --profile minimal``).

The baseline is generated with ``--omit blanket-impls --omit auto-trait-impls
--omit auto-derived-impls`` to focus on the intentionally-public API surface
rather than compiler-generated trait implementations.
"""

from __future__ import annotations

import argparse
import pathlib
import subprocess
import sys
import difflib

ROOT = pathlib.Path(__file__).resolve().parent.parent
BASELINE_DIR = ROOT / "api-baseline"

STABLE_CRATES = [
    "rill-ml",
    "rill-runtime-protocol",
    "rill-handler-api",
    "rill-runtime",
]

# All Stable crates are built with --all-features so the baseline covers
# every feature-gated public item (e.g. ``serde`` derives, ``wasm`` handler).
FEATURES = "--all-features"
OMIT = ["--omit", "blanket-impls", "--omit", "auto-trait-impls", "--omit", "auto-derived-impls"]


def run_cargo_public_api(crate: str) -> str:
    """Return the public API listing for *crate* as stdout text."""
    cmd = [
        "cargo", "public-api",
        "-p", crate,
        FEATURES,
        *OMIT,
    ]
    result = subprocess.run(
        cmd,
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        sys.stderr.write(
            f"error: cargo public-api failed for {crate} (exit {result.returncode})\n"
            f"stderr: {result.stderr}\n"
        )
        sys.exit(1)
    return result.stdout


def generate() -> None:
    BASELINE_DIR.mkdir(parents=True, exist_ok=True)
    for crate in STABLE_CRATES:
        output = run_cargo_public_api(crate)
        path = BASELINE_DIR / f"{crate}.txt"
        path.write_text(output, encoding="utf-8")
        print(f"  wrote {path.relative_to(ROOT)} ({len(output.splitlines())} lines)")


def verify() -> int:
    failures = 0
    for crate in STABLE_CRATES:
        path = BASELINE_DIR / f"{crate}.txt"
        if not path.exists():
            sys.stderr.write(f"error: baseline file missing: {path}\n")
            failures += 1
            continue
        expected = path.read_text(encoding="utf-8")
        actual = run_cargo_public_api(crate)
        if actual != expected:
            sys.stderr.write(
                f"FAIL: public API mismatch for {crate}\n"
                f"  baseline: {path.relative_to(ROOT)}\n"
                f"  run `python3 scripts/generate_api_baseline.py --generate` "
                f"to review the diff and update the baseline if intentional.\n"
            )
            diff = list(difflib.unified_diff(
                expected.splitlines(keepends=True),
                actual.splitlines(keepends=True),
                fromfile=str(path.relative_to(ROOT)),
                tofile=f"generated/{crate}.txt",
            ))
            sys.stderr.write(
                f"  baselineBytes={len(expected.encode())} generatedBytes={len(actual.encode())} "
                f"diffLines={len(diff)}\n"
            )
            sys.stderr.writelines(diff)
            if not diff:
                sys.stderr.write(f"  baselineTail={expected[-96:]!r}\n")
                sys.stderr.write(f"  generatedTail={actual[-96:]!r}\n")
            failures += 1
        else:
            print(f"  ok: {crate} matches baseline")
    return 0 if failures == 0 else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--generate", action="store_true",
                       help="Write the current public API to api-baseline/*.txt")
    group.add_argument("--verify", action="store_true",
                       help="Verify the current public API matches the frozen baseline")
    args = parser.parse_args()
    if args.generate:
        generate()
        return 0
    return verify()


if __name__ == "__main__":
    sys.exit(main())
