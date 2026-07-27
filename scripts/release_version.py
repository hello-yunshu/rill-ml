#!/usr/bin/env python3
"""Resolve and validate the release version for an automated release.

The release version is the Stable group version declared in
``release-plan.toml`` (e.g. ``1.0.0-rc.1`` for the 1.0 RC cycle, or
``1.0.0`` for the final stable release).  Preview crates (``rill-ml-python``,
``rill-ml-wasm``, adapters, ``rillml-inspect``) keep their own ``0.x``
version and are not the version that triggers an Auto Release tag.

``validate_release`` enforces:

* every Stable workspace crate is at the Stable group version;
* every Preview workspace crate is at the Preview group version;
* Stable internal path-dependency version requirements are
  ``^{stable_version}``;
* Preview crate path-dependency requirements on Stable crates are
  ``^{stable_version}``;
* ``models/example-default/manifest.json`` carries the Stable version;
* ``crates/rill-ml-python/pyproject.toml`` carries the Preview version;
* ``CHANGELOG.md`` has a dated release section for the Stable version.

The function returns the Stable version on success, which Auto Release
uses as the tag (``v{version}``) and GitHub Release title.  SemVer
prerelease identifiers (``1.0.0-rc.1``) are accepted and are not
normalised.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import subprocess
import sys
from typing import Any


# SemVer 2.0 — accepts optional prerelease and build metadata.
SEMVER_RE = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)


def cargo_metadata(root: pathlib.Path) -> dict[str, Any]:
    result = subprocess.run(
        [
            "cargo",
            "metadata",
            "--locked",
            "--no-deps",
            "--format-version",
            "1",
        ],
        cwd=root,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise ValueError(f"cargo metadata failed:\n{result.stderr.strip()}")
    return json.loads(result.stdout)


def load_release_plan(root: pathlib.Path) -> dict[str, dict[str, Any]]:
    """Parse ``release-plan.toml`` and return ``{group: {version, crates}}``.

    A minimal line-based parser is used to avoid a dependency on ``tomllib``
    (Python < 3.11) or ``tomli``.  The file format is intentionally simple
    and is shared with :mod:`sync_version`.  Multi-line ``crates`` arrays
    are supported.
    """
    plan_path = root / "release-plan.toml"
    if not plan_path.exists():
        raise ValueError(f"release-plan.toml not found at {plan_path}")
    text = plan_path.read_text(encoding="utf-8")
    plan: dict[str, dict[str, Any]] = {}
    current: str | None = None
    in_crates = False
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if in_crates:
            # Collect quoted crate names until the closing ']'.
            if line == "]":
                in_crates = False
                continue
            for name in re.findall(r'"([^"]+)"', line):
                assert current is not None
                plan[current]["crates"].append(name)
            continue
        section = re.match(r"^\[(\w+)\]$", line)
        if section:
            current = section.group(1)
            plan[current] = {"version": "", "crates": []}
            continue
        if current is None:
            continue
        version_match = re.match(r'^version\s*=\s*"([^"]+)"', line)
        if version_match:
            plan[current]["version"] = version_match.group(1)
            continue
        crates_match = re.match(r"^crates\s*=\s*\[(.*)\]$", line)
        if crates_match:
            # Single-line array like ``crates = ["a", "b"]``.
            inner = crates_match.group(1)
            for name in re.findall(r'"([^"]+)"', inner):
                plan[current]["crates"].append(name)
            continue
        crates_open = re.match(r"^crates\s*=\s*\[(.*)$", line)
        if crates_open:
            # Multi-line array opens here.  The remainder may be empty or
            # contain the start of the first value.
            inner = crates_open.group(1).strip()
            for name in re.findall(r'"([^"]+)"', inner):
                plan[current]["crates"].append(name)
            in_crates = True
    for group in ("stable", "preview"):
        if group not in plan:
            raise ValueError(f"release-plan.toml missing [{group}] section")
        if not plan[group]["version"]:
            raise ValueError(f"release-plan.toml [{group}] has no version")
    return plan


def project_version(pyproject: pathlib.Path) -> str:
    contents = pyproject.read_text(encoding="utf-8")
    project = re.search(r"(?ms)^\[project\]\s*(.*?)(?=^\[|\Z)", contents)
    if not project:
        raise ValueError(f"missing [project] table in {pyproject}")
    version = re.search(r'^version\s*=\s*"([^"]+)"\s*$', project.group(1), re.M)
    if not version:
        raise ValueError(f"missing project.version in {pyproject}")
    return version.group(1)


def validate_release(root: pathlib.Path, metadata: dict[str, Any]) -> str:
    """Return the Stable release version after validating group consistency."""
    plan = load_release_plan(root)
    stable_version = str(plan["stable"]["version"])
    preview_version = str(plan["preview"]["version"])
    stable_crates = set(plan["stable"]["crates"])
    preview_crates = set(plan["preview"]["crates"])
    if not SEMVER_RE.fullmatch(stable_version):
        raise ValueError(
            f"stable version {stable_version!r} is not valid SemVer"
        )
    if not SEMVER_RE.fullmatch(preview_version):
        raise ValueError(
            f"preview version {preview_version!r} is not valid SemVer"
        )

    workspace_ids = set(metadata["workspace_members"])
    packages = [
        package for package in metadata["packages"] if package["id"] in workspace_ids
    ]
    if not packages:
        raise ValueError("cargo metadata returned no workspace packages")

    workspace_names = {package["name"] for package in packages}

    # 1. Every workspace crate must be in the release plan, and must match
    #    its group's version exactly.
    for package in packages:
        name = package["name"]
        if name in stable_crates:
            expected = stable_version
            group = "stable"
        elif name in preview_crates:
            expected = preview_version
            group = "preview"
        else:
            raise ValueError(
                f"workspace crate {name!r} is not listed in release-plan.toml"
            )
        if package["version"] != expected:
            raise ValueError(
                f"{group} crate {name} is at {package['version']!r}, "
                f"expected {expected!r}"
            )

    # 2. Internal path-dependency version requirements on Stable crates
    #    must be ``^{stable_version}``.  Preview crates depend on Stable
    #    crates and must use the same requirement so that they track the
    #    Stable group without forcing a Preview bump.
    expected_req = f"^{stable_version}"
    for package in packages:
        for dependency in package.get("dependencies", []):
            if not dependency.get("path"):
                continue
            dep_name = dependency["name"]
            if dep_name not in workspace_names:
                continue
            if dep_name not in stable_crates:
                # Internal Preview-only deps do not exist today; if one is
                # added, sync_version.py must be extended.  For now, refuse.
                raise ValueError(
                    f"{package['name']} depends on internal Preview crate "
                    f"{dep_name}; cross-Preview path deps are not supported"
                )
            actual_req = dependency.get("req")
            if actual_req != expected_req:
                raise ValueError(
                    f"{package['name']} requires local {dep_name} at "
                    f"{actual_req!r}, expected {expected_req!r}"
                )

    # 3. Model manifest carries the Stable version (model packs are a
    #    Stable product contract).
    manifest_path = root / "models/example-default/manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    for field in ("version", "minRuntimeVersion"):
        if manifest.get(field) != stable_version:
            raise ValueError(
                f"{manifest_path}:{field} is {manifest.get(field)!r}, "
                f"expected {stable_version!r}"
            )

    # 4. Python pyproject carries the Preview version (Python is a Preview
    #    crate per the release plan).
    pyproject_path = root / "crates/rill-ml-python/pyproject.toml"
    python_version = project_version(pyproject_path)
    if python_version != preview_version:
        raise ValueError(
            f"{pyproject_path}:project.version is {python_version!r}, "
            f"expected preview {preview_version!r}"
        )

    # 5. CHANGELOG has a dated release section for the Stable version.  RC
    #    sections (``## [1.0.0-rc.1] - YYYY-MM-DD``) are explicitly allowed.
    changelog = (root / "CHANGELOG.md").read_text(encoding="utf-8")
    if not re.search(
        rf"(?m)^## \[{re.escape(stable_version)}\] - \d{{4}}-\d{{2}}-\d{{2}}$",
        changelog,
    ):
        raise ValueError(
            f"CHANGELOG.md has no dated release section for {stable_version}"
        )

    return stable_version


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=pathlib.Path, default=pathlib.Path.cwd())
    parser.add_argument("--github-output", type=pathlib.Path)
    args = parser.parse_args()

    try:
        root = args.root.resolve()
        version = validate_release(root, cargo_metadata(root))
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"release version validation failed: {error}", file=sys.stderr)
        return 1

    tag = f"v{version}"
    print(f"release version {version} is internally consistent ({tag})")
    if args.github_output:
        with args.github_output.open("a", encoding="utf-8") as output:
            output.write(f"version={version}\n")
            output.write(f"tag={tag}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
