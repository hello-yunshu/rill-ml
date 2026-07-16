#!/usr/bin/env python3
"""Synchronise the canonical workspace version to every static file.

The single source of truth is ``[workspace.package] version`` in the root
``Cargo.toml``.  After editing that one field, run::

    python3 scripts/sync_version.py

and the script propagates the version to every file that cannot inherit it
at compile time (Python metadata, JSON manifests, excluded handler crates,
documentation, CHANGELOG skeleton, and the ``[workspace.dependencies]``
internal-version requirements).

Rust source files and integration tests use ``env!("CARGO_PKG_VERSION")``
and therefore do **not** need to be touched by this script (or by a version
bump).

The script is idempotent — running it twice produces no additional changes.
"""

from __future__ import annotations

import datetime
import json
import pathlib
import re
import subprocess
import sys

SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+$")


# --------------------------------------------------------------------------- #
#  Version source                                                             #
# --------------------------------------------------------------------------- #

def workspace_version(root: pathlib.Path) -> str:
    """Return the canonical version from ``cargo metadata``."""
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        cwd=root,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(f"cargo metadata failed:\n{result.stderr.strip()}")
    metadata = json.loads(result.stdout)
    versions = {
        pkg["version"]
        for pkg in metadata["packages"]
        if pkg["id"] in set(metadata["workspace_members"])
    }
    if len(versions) != 1:
        raise RuntimeError(f"workspace versions disagree: {versions}")
    version = versions.pop()
    if not SEMVER_RE.match(version):
        raise RuntimeError(f"version {version!r} is not stable SemVer x.y.z")
    return version


# --------------------------------------------------------------------------- #
#  Individual sync targets                                                    #
# --------------------------------------------------------------------------- #

def _replace(pattern: re.Pattern[str], replacement: str, text: str) -> tuple[str, int]:
    new_text, count = pattern.subn(replacement, text)
    if new_text == text:
        return text, 0
    return new_text, count


def sync_workspace_deps(cargo_toml: pathlib.Path, version: str) -> int:
    """Update internal-crate version requirements in ``[workspace.dependencies]``."""
    text = cargo_toml.read_text(encoding="utf-8")
    # Only touch lines that declare an internal crate (rill-*) with a version.
    pattern = re.compile(
        r'(?m)^((?:rill-handler-api|rill-ml|rill-runtime-protocol)\s*=\s*\{[^}]*?version\s*=\s*")[^"]+(")'
    )
    new_text, count = _replace(pattern, rf'\g<1>{version}\g<2>', text)
    if count:
        cargo_toml.write_text(new_text, encoding="utf-8")
    return count


def sync_pyproject(pyproject: pathlib.Path, version: str) -> int:
    text = pyproject.read_text(encoding="utf-8")
    pattern = re.compile(r'(?m)^version\s*=\s*"[^"]+"')
    new_text, count = _replace(pattern, f'version = "{version}"', text)
    if count:
        pyproject.write_text(new_text, encoding="utf-8")
    return count


def sync_json_manifest(manifest: pathlib.Path, version: str) -> int:
    """Update ``version`` and ``minRuntimeVersion`` (or ``min_runtime_version``)."""
    text = manifest.read_text(encoding="utf-8")
    data = json.loads(text)
    count = 0
    for field in ("version", "minRuntimeVersion"):
        if field in data and data[field] != version:
            data[field] = version
            count += 1
    if count:
        manifest.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return count


def sync_handler_cargo_toml(cargo_toml: pathlib.Path, version: str) -> int:
    text = cargo_toml.read_text(encoding="utf-8")
    pattern = re.compile(r'(?m)^version\s*=\s*"[^"]+"')
    new_text, count = _replace(pattern, f'version = "{version}"', text)
    if count:
        cargo_toml.write_text(new_text, encoding="utf-8")
    return count


def sync_roadmap(roadmap: pathlib.Path, version: str) -> int:
    """Update the ``状态：当前（vX.Y.Z，YYYY-MM-DD）`` status line."""
    text = roadmap.read_text(encoding="utf-8")
    today = datetime.date.today().isoformat()
    pattern = re.compile(
        r"(> 状态：当前（v)\d+\.\d+\.\d+(，)\d{4}-\d{2}-\d{2}(）)"
    )
    new_text, count = _replace(pattern, rf"\g<1>{version}\g<2>{today}\g<3>", text)
    if count:
        roadmap.write_text(new_text, encoding="utf-8")
    return count


def sync_security(security: pathlib.Path, version: str) -> int:
    """Update the supported-versions table to the new minor line."""
    text = security.read_text(encoding="utf-8")
    minor = re.match(r"^(\d+)\.(\d+)\.", version)
    if not minor:
        return 0
    minor_line = f"{minor.group(1)}.{minor.group(2)}.x"
    count = 0
    # Replace the supported row: "| 0.X.x   | :white_check_mark: |"
    pattern_supported = re.compile(
        r"(^\| )\d+\.\d+\.x(\s*\| :white_check_mark: \|)", re.MULTILINE
    )
    new_text, n = _replace(pattern_supported, rf"\g<1>{minor_line}\g<2>", text)
    count += n
    # Replace the floor row: "| < 0.X   | :x:                |"
    pattern_floor = re.compile(
        r"(^\| < )\d+\.\d+(\s*\| :x:)", re.MULTILINE
    )
    new_text, n = _replace(pattern_floor, rf"\g<1>{minor.group(1)}.{minor.group(2)}\g<2>", new_text)
    count += n
    if count:
        security.write_text(new_text, encoding="utf-8")
    return count


def sync_changelog(changelog: pathlib.Path, version: str) -> int:
    """Add a dated ``## [version] - YYYY-MM-DD`` skeleton and link references."""
    text = changelog.read_text(encoding="utf-8")
    today = datetime.date.today().isoformat()
    repo = "https://github.com/hello-yunshu/rill-ml"
    count = 0

    # 1. Add the section skeleton if absent.
    header = f"## [{version}] - {today}"
    if not re.search(rf"(?m)^## \[{re.escape(version)}\] - \d{{4}}-\d{{2}}-\d{{2}}$", text):
        skeleton = (
            f"{header}\n\n"
            f"### Changed\n\n"
            f"- TODO: describe notable changes for {version}.\n\n"
        )
        pattern = re.compile(r"(## \[Unreleased\]\s*\n)")
        text, n = _replace(pattern, rf"\g<1>\n{skeleton}", text)
        count += n

    # 2. Update the [Unreleased] comparison link to point from the new version.
    pattern_unreleased = re.compile(
        r"(^\[Unreleased\]: )https://[^/]+/[^/]+/compare/v\d+\.\d+\.\d+\.\.\.HEAD",
        re.MULTILINE,
    )
    text, n = _replace(pattern_unreleased, rf"\g<1>{repo}/compare/v{version}...HEAD", text)
    count += n

    # 3. Add the [version] tag link if absent.
    tag_link = f"[{version}]: {repo}/releases/tag/v{version}"
    if not re.search(rf"(?m)^\[{re.escape(version)}\]:", text):
        # Insert right after the [Unreleased] line.
        pattern = re.compile(r"(^\[Unreleased\]: [^\n]+\n)", re.MULTILINE)
        text, n = _replace(pattern, rf"\g<1>{tag_link}\n", text)
        count += n

    if count:
        changelog.write_text(text, encoding="utf-8")
    return count


# --------------------------------------------------------------------------- #
#  Orchestration                                                              #
# --------------------------------------------------------------------------- #

def main() -> int:
    root = pathlib.Path(__file__).resolve().parent.parent

    try:
        version = workspace_version(root)
    except RuntimeError as error:
        print(f"sync: {error}", file=sys.stderr)
        return 1

    print(f"sync: target version {version}")

    targets: list[tuple[str, pathlib.Path, int]] = []

    # 1. [workspace.dependencies] internal dep versions in root Cargo.toml.
    targets.append((
        "Cargo.toml [workspace.dependencies]",
        root / "Cargo.toml",
        sync_workspace_deps(root / "Cargo.toml", version),
    ))

    # 2. Python pyproject.toml.
    targets.append((
        "pyproject.toml",
        root / "crates/rill-ml-python/pyproject.toml",
        sync_pyproject(root / "crates/rill-ml-python/pyproject.toml", version),
    ))

    # 3. Model manifest.
    targets.append((
        "models/example-default/manifest.json",
        root / "models/example-default/manifest.json",
        sync_json_manifest(root / "models/example-default/manifest.json", version),
    ))

    # 4. Echo handler (excluded from workspace).
    targets.append((
        "handlers/echo-handler/Cargo.toml",
        root / "handlers/echo-handler/Cargo.toml",
        sync_handler_cargo_toml(root / "handlers/echo-handler/Cargo.toml", version),
    ))
    targets.append((
        "handlers/echo-handler/manifest.json",
        root / "handlers/echo-handler/manifest.json",
        sync_json_manifest(root / "handlers/echo-handler/manifest.json", version),
    ))

    # 5. Test malicious handler (excluded from workspace).
    targets.append((
        "handlers/test-malicious-handler/Cargo.toml",
        root / "handlers/test-malicious-handler/Cargo.toml",
        sync_handler_cargo_toml(root / "handlers/test-malicious-handler/Cargo.toml", version),
    ))

    # 6. Documentation.
    targets.append((
        "ROADMAP.md",
        root / "ROADMAP.md",
        sync_roadmap(root / "ROADMAP.md", version),
    ))
    targets.append((
        "SECURITY.md",
        root / "SECURITY.md",
        sync_security(root / "SECURITY.md", version),
    ))

    # 7. CHANGELOG skeleton.
    targets.append((
        "CHANGELOG.md (skeleton)",
        root / "CHANGELOG.md",
        sync_changelog(root / "CHANGELOG.md", version),
    ))

    # Summary.
    print()
    changed = 0
    for label, _, count in targets:
        status = f"{count} update(s)" if count else "ok"
        print(f"  {label:50s} {status}")
        changed += count

    print(f"\nsync: {changed} field(s) updated for version {version}")
    print("sync: remember to fill in CHANGELOG.md release notes before releasing.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
