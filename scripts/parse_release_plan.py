#!/usr/bin/env python3
"""Parse ``release-plan.toml`` and emit the Stable publish order.

This script is the single source of truth for which crates to publish during
a release.  It reads ``release-plan.toml`` and outputs the Stable crate list
in publish order (one per line), with full validation:

- No duplicate crates in any group.
- No unknown crates (validated against ``cargo metadata`` workspace members).
- Stable group must be non-empty.
- Stable version must be valid SemVer.
- When ``--tag`` is provided, the Stable version must match the tag version
  (after stripping the ``v`` prefix).

Used by the Release workflow's package and publish jobs. Only the Stable group
is packaged and published; Preview crates are never published by this release
plan.

Usage::

    python3 scripts/parse_release_plan.py --list-stable
    python3 scripts/parse_release_plan.py --list-stable-paths
    python3 scripts/parse_release_plan.py --list-stable --tag v1.0.0-rc.6
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import subprocess
import sys

# Import the shared parser from sync_version to avoid duplication.
SCRIPTS = pathlib.Path(__file__).resolve().parent
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import sync_version  # noqa: E402  (path inserted above)

# SemVer 2.0 pattern that accepts optional prerelease and build metadata.
SEMVER_RE = re.compile(
    r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)


def workspace_member_names(root: pathlib.Path) -> set[str]:
    """Return the set of workspace member crate names via ``cargo metadata``."""
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
    return {pkg["name"] for pkg in metadata["packages"] if pkg["id"] in set(metadata["workspace_members"])}


def workspace_member_paths(root: pathlib.Path) -> dict[str, pathlib.Path]:
    """Return workspace crate names mapped to repository-relative directories."""
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
    workspace_members = set(metadata["workspace_members"])
    paths: dict[str, pathlib.Path] = {}
    for package in metadata["packages"]:
        if package["id"] not in workspace_members:
            continue
        manifest_dir = pathlib.Path(package["manifest_path"]).resolve().parent
        try:
            relative = manifest_dir.relative_to(root.resolve())
        except ValueError as error:
            raise RuntimeError(
                f"workspace crate {package['name']!r} is outside repository root"
            ) from error
        paths[package["name"]] = relative
    return paths


def stable_package_paths(
    root: pathlib.Path, stable_crates: list[str]
) -> list[tuple[str, pathlib.Path]]:
    """Return Stable crates and their repository-relative package directories."""
    known_paths = workspace_member_paths(root)
    result: list[tuple[str, pathlib.Path]] = []
    for name in stable_crates:
        if name not in known_paths:
            raise RuntimeError(
                f"release-plan.toml [stable] references unknown crate {name!r} "
                "(no workspace package path)"
            )
        result.append((name, known_paths[name]))
    return result


def validate_release_plan(root: pathlib.Path, tag_version: str | None = None) -> list[str]:
    """Validate ``release-plan.toml`` and return the Stable crate list.

    Args:
        root: Repository root path.
        tag_version: Expected version (without ``v`` prefix) from the release
            tag.  If provided, the Stable version must match exactly.

    Returns:
        The Stable crate list in publish order.

    Raises:
        RuntimeError: If any validation check fails.
    """
    plan = sync_version.load_release_plan(root)

    stable = plan["stable"]  # type: ignore[index]
    preview = plan["preview"]  # type: ignore[index]

    stable_crates: list[str] = list(stable["crates"])  # type: ignore[union-attr]
    preview_crates: list[str] = list(preview["crates"])  # type: ignore[union-attr]
    stable_version: str = str(stable["version"])  # type: ignore[index]

    # --- Non-empty Stable ------------------------------------------------- #
    if not stable_crates:
        raise RuntimeError("release-plan.toml [stable] has an empty crates list")

    # --- No duplicates ----------------------------------------------------- #
    seen_stable: set[str] = set()
    for name in stable_crates:
        if name in seen_stable:
            raise RuntimeError(f"release-plan.toml [stable] has duplicate crate '{name}'")
        seen_stable.add(name)

    seen_preview: set[str] = set()
    for name in preview_crates:
        if name in seen_preview:
            raise RuntimeError(f"release-plan.toml [preview] has duplicate crate '{name}'")
        seen_preview.add(name)

    # --- No overlap between Stable and Preview ---------------------------- #
    overlap = seen_stable & seen_preview
    if overlap:
        raise RuntimeError(
            f"release-plan.toml: crates appear in both stable and preview: {sorted(overlap)}"
        )

    # --- Valid SemVer ------------------------------------------------------ #
    if not SEMVER_RE.match(stable_version):
        raise RuntimeError(
            f"release-plan.toml [stable] version {stable_version!r} is not valid SemVer"
        )

    # --- Version matches tag (if provided) -------------------------------- #
    if tag_version is not None and tag_version != stable_version:
        raise RuntimeError(
            f"release-plan.toml [stable] version {stable_version!r} does not match "
            f"release tag version {tag_version!r}"
        )

    # --- All crates are known workspace members --------------------------- #
    known = workspace_member_names(root)
    for name in stable_crates:
        if name not in known:
            raise RuntimeError(
                f"release-plan.toml [stable] references unknown crate '{name}' "
                f"(not a workspace member)"
            )
    for name in preview_crates:
        if name not in known:
            raise RuntimeError(
                f"release-plan.toml [preview] references unknown crate '{name}' "
                f"(not a workspace member)"
            )

    return stable_crates


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Parse release-plan.toml and emit the Stable publish order."
    )
    parser.add_argument(
        "--list-stable",
        action="store_true",
        help="Print the Stable crate list in publish order (one per line).",
    )
    parser.add_argument(
        "--list-preview",
        action="store_true",
        help="Print the Preview crate list (for verification that Preview is not published).",
    )
    parser.add_argument(
        "--list-stable-paths",
        action="store_true",
        help=(
            "Print each Stable crate and its repository-relative package "
            "directory, separated by a tab."
        ),
    )
    parser.add_argument(
        "--tag",
        type=str,
        default=None,
        help="Release tag (e.g. v1.0.0-rc.6) to validate against the Stable version.",
    )
    args = parser.parse_args()

    root = pathlib.Path(__file__).resolve().parent.parent

    tag_version: str | None = None
    if args.tag is not None:
        tag_version = args.tag.removeprefix("v")

    try:
        stable_crates = validate_release_plan(root, tag_version=tag_version)
    except RuntimeError as error:
        print(f"parse_release_plan: {error}", file=sys.stderr)
        return 1

    if args.list_stable:
        for name in stable_crates:
            print(name)
    if args.list_stable_paths:
        try:
            package_paths = stable_package_paths(root, stable_crates)
        except RuntimeError as error:
            print(f"parse_release_plan: {error}", file=sys.stderr)
            return 1
        for name, path in package_paths:
            print(f"{name}\t{path if path.parts else '.'}")
    if args.list_preview:
        plan = sync_version.load_release_plan(root)
        for name in plan["preview"]["crates"]:  # type: ignore[index]
            print(name)

    if not args.list_stable and not args.list_preview and not args.list_stable_paths:
        print("parse_release_plan: validation passed.")
        print(f"  Stable version: {sync_version.load_release_plan(root)['stable']['version']}")  # type: ignore[index]
        print(f"  Stable crates:  {stable_crates}")
        print(f"  Preview crates: {sync_version.load_release_plan(root)['preview']['crates']}")  # type: ignore[index]

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
