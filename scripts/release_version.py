#!/usr/bin/env python3
"""Resolve and validate the single version used by an automated release."""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import subprocess
import sys
from typing import Any


SEMVER_RE = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")


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
    workspace_ids = set(metadata["workspace_members"])
    packages = [
        package for package in metadata["packages"] if package["id"] in workspace_ids
    ]
    if not packages:
        raise ValueError("cargo metadata returned no workspace packages")

    versions = {package["version"] for package in packages}
    if len(versions) != 1:
        details = ", ".join(
            f"{package['name']}={package['version']}" for package in packages
        )
        raise ValueError(f"workspace package versions disagree: {details}")
    version = versions.pop()
    if not SEMVER_RE.fullmatch(version):
        raise ValueError(f"release version must be stable SemVer x.y.z, got {version}")

    workspace_names = {package["name"] for package in packages}
    expected_req = f"^{version}"
    for package in packages:
        for dependency in package.get("dependencies", []):
            if dependency.get("path") and dependency["name"] in workspace_names:
                if dependency.get("req") != expected_req:
                    raise ValueError(
                        f"{package['name']} requires local {dependency['name']} "
                        f"at {dependency.get('req')}, expected {expected_req}"
                    )

    manifest_path = root / "models/example-default/manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    for field in ("version", "minRuntimeVersion"):
        if manifest.get(field) != version:
            raise ValueError(
                f"{manifest_path}:{field} is {manifest.get(field)!r}, expected {version!r}"
            )

    pyproject_path = root / "crates/rill-ml-python/pyproject.toml"
    python_version = project_version(pyproject_path)
    if python_version != version:
        raise ValueError(
            f"{pyproject_path}:project.version is {python_version!r}, expected {version!r}"
        )

    changelog = (root / "CHANGELOG.md").read_text(encoding="utf-8")
    if not re.search(rf"(?m)^## \[{re.escape(version)}\] - \d{{4}}-\d{{2}}-\d{{2}}$", changelog):
        raise ValueError(f"CHANGELOG.md has no dated release section for {version}")

    return version


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
