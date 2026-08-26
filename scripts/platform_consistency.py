#!/usr/bin/env python3
"""Fail-closed consistency checks for platform support surfaces."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

from platform_support import load_platforms


TARGET_RE = re.compile(r"`([A-Za-z0-9_]+-[A-Za-z0-9_.-]+)`")


def check(name: str, condition: bool, detail: str) -> dict[str, str]:
    return {"name": name, "status": "PASS" if condition else "FAIL", "detail": detail}


def supported_rows(markdown: str) -> dict[str, str]:
    section = markdown.split("## Supported Platforms", 1)[1].split("### Notes", 1)[0]
    rows: dict[str, str] = {}
    for line in section.splitlines():
        if not line.startswith("|") or "Target" in line or "---" in line:
            continue
        match = TARGET_RE.search(line)
        if match:
            rows[match.group(1)] = line
    return rows


def run(root: Path) -> dict[str, object]:
    platforms = load_platforms(root)
    entries = {str(entry["triple"]): entry for entry in platforms}
    supported = {triple for triple, entry in entries.items() if entry["core_supported"]}
    runtime = {triple for triple, entry in entries.items() if entry["runtime_supported"]}
    docs = (root / "PLATFORM_SUPPORT.md").read_text(encoding="utf-8")
    rows = supported_rows(docs)
    builder = (root / "scripts/build-release-index.py").read_text(encoding="utf-8")
    pipeline = (root / ".github/workflows/pipeline.yml").read_text(encoding="utf-8")
    cross = (root / ".github/workflows/cross-platform.yml").read_text(encoding="utf-8")
    musl = (root / ".github/workflows/linux-musl.yml").read_text(encoding="utf-8")
    post_release = (root / "scripts/post-release-qemu-verify.sh").read_text(encoding="utf-8")

    checks = [
        check(
            "musl.backend-map",
            all(
                entry.get("runtime_backend") in {"cranelift", "pulley32", "pulley32be"}
                and entry.get("pointer_width") in {32, 64}
                and entry.get("endianness") in {"little", "big"}
                for entry in entries.values()
                if entry["target_libc"] == "musl"
            ),
            "every musl target records backend, pointer width, and endianness",
        ),
        check(
            "docs.core-targets",
            set(rows) == supported,
            f"documentation rows={sorted(rows)}, registry Core={sorted(supported)}",
        ),
        check(
            "docs.runtime-surface",
            all(("✅" in rows[t] if entries[t]["runtime_supported"] else "not listed" in rows[t]) for t in rows),
            "Runtime column agrees with the registry for every documented target",
        ),
        check(
            "runtime.release-index",
            all(
                entry["target_arch"] in builder and entry["asset_suffix"] in builder
                for triple, entry in entries.items()
                if triple in runtime
            ),
            "every Full Runtime target has a deterministic release-index asset pattern",
        ),
        check(
            "runtime.core-only-release-assets",
            all(
                entry["release_asset"] is False
                for entry in entries.values()
                if not entry["runtime_supported"]
            ),
            "Core-only targets are excluded from Stable Release assets",
        ),
        check(
            "runtime.source-gate",
            all(triple in cross or triple in pipeline or triple in musl for triple in runtime),
            "every Full Runtime target is present in a CI source gate",
        ),
        check(
            "runtime.post-release-gate",
            all(
                triple in pipeline
                or (entry["post_release_gate"] in {"direct-qemu", "direct-qemu-musl"} and triple in post_release)
                for triple, entry in entries.items()
                if entry["runtime_supported"]
            ),
            "every Full Runtime target is present in a post-release verification path",
        ),
        check(
            "no-stale-gate-comments",
            "armv7 stays Not listed" not in cross and "FreeBSD is Not yet claimed Stable" not in cross,
            "workflow comments do not contradict the registry",
        ),
        check(
            "default-runtime-feature",
            all(
                re.search(
                    rf"- target: {re.escape(triple)}\s+runtime_features:\s*--no-default-features",
                    cross,
                )
                is None
                for triple in runtime
            ),
            "Full Runtime source gates do not use the WASM-free feature set",
        ),
    ]
    return {
        "status": "PASS" if all(item["status"] == "PASS" for item in checks) else "FAIL",
        "checks": checks,
        "coreTargets": sorted(supported),
        "runtimeTargets": sorted(runtime),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    result = run(args.root.resolve())
    if args.json:
        print(json.dumps(result, ensure_ascii=False, sort_keys=True, separators=(",", ":")))
    else:
        for item in result["checks"]:
            print(f"{item['status']} {item['name']}: {item['detail']}")
        print(result["status"] + " platform-consistency")
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
