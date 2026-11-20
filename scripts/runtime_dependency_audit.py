#!/usr/bin/env python3
"""Audit the shipped Runtime dependency graph without hiding workspace findings.

The workspace lock also contains optional analytics crates (polars/zstd) that
are never compiled into `rill-runtime`. The shipped Runtime graph must audit
clean. The full workspace audit is retained separately as risk evidence; a new
dependency entering the shipped graph makes this gate fail closed.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = "x86_64-unknown-linux-musl"
OPTIONAL_ANALYTICS = {"polars", "zstd", "zstd-safe", "zstd-sys"}


def shipped_runtime_lock(lock_text: str, packages: set[tuple[str, str]]) -> str:
    records = {}
    for block in re.split(r"(?m)^\[\[package\]\]\n", lock_text)[1:]:
        match = re.match(r'name = "([^"]+)"\nversion = "([^"]+)"', block)
        if not match:
            raise ValueError("invalid Cargo.lock package record")
        dependencies = []
        listed = re.search(r"(?m)^dependencies = \[\n(.*?)\n\]", block, re.S)
        if listed:
            for line in listed.group(1).splitlines():
                entry = re.match(r'\s*"([^"]+)"', line)
                if entry:
                    dependencies.append(entry.group(1))
        records[(match.group(1), match.group(2))] = (block, dependencies)

    keep = set(packages)

    def resolve(spec):
        fields = spec.split()
        return [(name, version) for (name, version) in records if name == fields[0] and (len(fields) == 1 or version == fields[1])]

    changed = True
    while changed:
        changed = False
        for key in list(keep):
            if key not in records:
                raise ValueError("shipped Runtime dependency is missing from Cargo.lock: " + str(key))
            for dependency in records[key][1]:
                for target in resolve(dependency):
                    if target not in keep:
                        keep.add(target)
                        changed = True
    return "version = 3\n\n" + "".join("[[package]]\n" + records[key][0] for key in sorted(keep))


def run_audit(lock: Path, output: Path, stem: str):
    completed = subprocess.run(
        ["cargo", "audit", "--file", str(lock), "--json"],
        cwd=ROOT, text=True, capture_output=True, timeout=300,
    )
    (output / f"{stem}.json").write_text(completed.stdout)
    (output / f"{stem}.stderr.log").write_text(completed.stderr)
    try:
        parsed = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise ValueError(f"{stem}: cargo audit did not return valid JSON") from error
    if re.search(r"(?m)^error:", completed.stderr):
        raise ValueError(f"{stem}: cargo audit could not inspect the dependency index")
    return completed.returncode, parsed


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output
    output.mkdir(parents=True, exist_ok=True)
    tree = subprocess.check_output(
        ["cargo", "tree", "--locked", "--edges", "normal,build", "-p", "rill-runtime",
         "--target", TARGET, "--prefix", "none", "--format", "{p}|{f}"],
        cwd=ROOT, text=True,
    )
    packages = set()
    for line in tree.splitlines():
        record = line.removesuffix(" (*)").split("|", 1)[0].strip()
        fields = record.split()
        if len(fields) >= 2 and fields[1].startswith("v"):
            packages.add((fields[0], fields[1][1:]))
    if not packages:
        raise ValueError("shipped Runtime package graph is empty")
    leaked = sorted(name for name, _ in packages if name in OPTIONAL_ANALYTICS)
    if leaked:
        raise ValueError("optional analytics dependency leaked into the shipped Runtime graph: " + ", ".join(leaked))

    lock = ROOT / "Cargo.lock"
    filtered = output / "shipped-runtime-Cargo.lock"
    filtered.write_text(shipped_runtime_lock(lock.read_text(), packages))
    workspace_rc, workspace = run_audit(lock, output, "workspace-audit")
    shipped_rc, shipped = run_audit(filtered, output, "shipped-runtime-audit")
    if shipped_rc or shipped.get("vulnerabilities", {}).get("count") != 0 or shipped.get("warnings") or shipped.get("settings", {}).get("ignore"):
        raise ValueError("shipped Runtime dependency graph has unresolved or ignored security findings")

    workspace_findings = sorted({
        item.get("advisory", {}).get("id", "")
        for item in workspace.get("vulnerabilities", {}).get("list", [])
        if item.get("advisory", {}).get("id")
    })
    shipped_summary = {
        "graph": "shipped-runtime-default-features",
        "target": TARGET,
        "dependencyCount": shipped.get("lockfile", {}).get("dependency-count", 0),
        "database": shipped.get("database", {}),
        "vulnerabilityCount": 0,
        "warnings": {},
        "ignoredAdvisories": [],
    }
    workspace_summary = {
        "dependencyCount": workspace.get("lockfile", {}).get("dependency-count", 0),
        "database": workspace.get("database", {}),
        "vulnerabilityCount": workspace.get("vulnerabilities", {}).get("count", 0),
        "advisoryIds": workspace_findings,
        "warnings": sorted(workspace.get("warnings", {})),
        "ignoredAdvisories": workspace.get("settings", {}).get("ignore", []),
        "reachableInShippedRuntime": False,
        "reason": "optional analytics workspace dependencies are absent from the shipped rill-runtime cargo tree",
    }
    (output / "shipped-runtime-summary.json").write_text(json.dumps(shipped_summary, indent=2) + "\n")
    (output / "workspace-summary.json").write_text(json.dumps(workspace_summary, indent=2) + "\n")
    print(json.dumps({"shippedRuntime": shipped_summary, "workspace": workspace_summary}, indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
