#!/usr/bin/env python3
"""Synchronise the canonical workspace version to every static file.

The single source of truth is ``[workspace.package] version`` in the root
``Cargo.toml`` for the Stable group, and ``release-plan.toml`` for the
Preview group version.  After editing those fields, run::

    python3 scripts/sync_version.py

and the script propagates the version to every file that cannot inherit it
at compile time (Python metadata, JSON manifests, excluded handler crates,
documentation, CHANGELOG skeleton, and the ``[workspace.dependencies]``
internal-version requirements).

Stable crates (rill-ml, rill-runtime-protocol, rill-handler-api,
rill-runtime) follow ``[workspace.package] version`` and may carry SemVer
prerelease identifiers such as ``1.0.0-rc.1``.

Preview crates (rill-ml-python, rill-ml-wasm, rill-ml-tokio,
rill-ml-arrow, rill-ml-polars, rillml-inspect) keep their own explicit
``version = "..."`` in their ``Cargo.toml`` and stay at ``0.x`` per
``release-plan.toml`` until they are promoted to Stable.

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

# SemVer 2.0 pattern that accepts optional prerelease and build metadata.
SEMVER_RE = re.compile(
    r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)


# --------------------------------------------------------------------------- #
#  Release plan                                                               #
# --------------------------------------------------------------------------- #

def load_release_plan(root: pathlib.Path) -> dict[str, object]:
    """Parse ``release-plan.toml`` and return ``{group: {version, crates}}``.

    A minimal line-based parser is used to avoid a dependency on ``tomllib``
    (Python < 3.11) or ``tomli``.  The file format is intentionally simple.
    Multi-line ``crates`` arrays are supported.
    """
    plan_path = root / "release-plan.toml"
    if not plan_path.exists():
        raise RuntimeError(f"release-plan.toml not found at {plan_path}")
    text = plan_path.read_text(encoding="utf-8")
    plan: dict[str, object] = {}
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
                plan[current]["crates"].append(name)  # type: ignore[union-attr]
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
            plan[current]["version"] = version_match.group(1)  # type: ignore[index]
            continue
        crates_match = re.match(r"^crates\s*=\s*\[(.*)\]$", line)
        if crates_match:
            # Single-line array like ``crates = ["a", "b"]``.
            inner = crates_match.group(1)
            for name in re.findall(r'"([^"]+)"', inner):
                plan[current]["crates"].append(name)  # type: ignore[union-attr]
            continue
        crates_open = re.match(r"^crates\s*=\s*\[(.*)$", line)
        if crates_open:
            # Multi-line array opens here.  The remainder may be empty or
            # contain the start of the first value.
            inner = crates_open.group(1).strip()
            for name in re.findall(r'"([^"]+)"', inner):
                plan[current]["crates"].append(name)  # type: ignore[union-attr]
            in_crates = True
    for group in ("stable", "preview"):
        if group not in plan:
            raise RuntimeError(f"release-plan.toml missing [{group}] section")
        if not plan[group]["version"]:  # type: ignore[index]
            raise RuntimeError(f"release-plan.toml [{group}] has no version")
    return plan


# --------------------------------------------------------------------------- #
#  Version source                                                             #
# --------------------------------------------------------------------------- #

def workspace_version(root: pathlib.Path) -> str:
    """Return the Stable group version from ``[workspace.package]``.

    This is the version applied to Stable crates.  Preview crates are
    verified separately by :func:`verify_workspace_versions`.
    """
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
    plan = load_release_plan(root)
    stable_crates = set(plan["stable"]["crates"])  # type: ignore[index]
    preview_crates = set(plan["preview"]["crates"])  # type: ignore[index]
    stable_version = str(plan["stable"]["version"])  # type: ignore[index]
    preview_version = str(plan["preview"]["version"])  # type: ignore[index]
    for pkg in metadata["packages"]:
        if pkg["id"] not in set(metadata["workspace_members"]):
            continue
        name = pkg["name"]
        if name in stable_crates:
            if pkg["version"] != stable_version:
                raise RuntimeError(
                    f"stable crate {name} is at {pkg['version']}, "
                    f"expected {stable_version}"
                )
        elif name in preview_crates:
            if pkg["version"] != preview_version:
                raise RuntimeError(
                    f"preview crate {name} is at {pkg['version']}, "
                    f"expected {preview_version}"
                )
    if not SEMVER_RE.match(stable_version):
        raise RuntimeError(
            f"stable version {stable_version!r} is not valid SemVer"
        )
    if not SEMVER_RE.match(preview_version):
        raise RuntimeError(
            f"preview version {preview_version!r} is not valid SemVer"
        )
    return stable_version


def preview_version(root: pathlib.Path) -> str:
    """Return the Preview group version from ``release-plan.toml``."""
    plan = load_release_plan(root)
    return str(plan["preview"]["version"])  # type: ignore[index]


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


def verify_readme_no_hardcoded_version(readme: pathlib.Path) -> int:
    """Return the number of hardcoded ``rill-ml = "0.x"`` dependency lines.

    The README installation examples use ``cargo add rill-ml`` so that the
    version never drifts from ``[workspace.package].version``. This check
    is a regression guard: if a future edit reintroduces a literal
    ``rill-ml = "0.x"`` TOML dependency line, ``sync_version.py`` will
    report it and the CI unittest fails.

    Roadmap bullets such as ``- **v0.7** —`` are intentionally not matched
    because they describe historical feature lines, not the current
    install command.
    """
    text = readme.read_text(encoding="utf-8")
    # Match two TOML dependency forms that drift from the canonical
    # workspace version:
    #   1. ``rill-ml = "0.7"`` / ``rill-ml = "0.7.0"`` / ``rill-ml = 0.7``
    #   2. ``rill-ml = { version = "0.7", features = ["serde"] }``
    # Roadmap bullets (``- **v0.7** —``) are not preceded by ``rill-ml =``
    # and are therefore intentionally not matched.
    simple = re.compile(r'(?m)^\s*rill-ml\s*=\s*"?0\.\d+(?:\.\d+)?"?')
    inline_table = re.compile(
        r'(?m)^\s*rill-ml\s*=\s*\{[^}]*\bversion\s*=\s*"0\.\d+(?:\.\d+)?"'
    )
    matched_lines: set[int] = set()
    for match in simple.finditer(text):
        matched_lines.add(match.start())
    for match in inline_table.finditer(text):
        matched_lines.add(match.start())
    return len(matched_lines)


# --------------------------------------------------------------------------- #
#  Orchestration                                                              #
# --------------------------------------------------------------------------- #

def main() -> int:
    root = pathlib.Path(__file__).resolve().parent.parent

    try:
        stable = workspace_version(root)
        preview = preview_version(root)
    except RuntimeError as error:
        print(f"sync: {error}", file=sys.stderr)
        return 1

    print(f"sync: stable version {stable}, preview version {preview}")

    targets: list[tuple[str, pathlib.Path, int]] = []

    # 1. [workspace.dependencies] internal dep versions in root Cargo.toml
    #    (Stable crates only — Preview crates keep explicit versions).
    targets.append((
        "Cargo.toml [workspace.dependencies]",
        root / "Cargo.toml",
        sync_workspace_deps(root / "Cargo.toml", stable),
    ))

    # 2. Python pyproject.toml — Python is a Preview crate.
    targets.append((
        "pyproject.toml (preview)",
        root / "crates/rill-ml-python/pyproject.toml",
        sync_pyproject(root / "crates/rill-ml-python/pyproject.toml", preview),
    ))

    # 3. Model manifest — runtime is Stable.
    targets.append((
        "models/example-default/manifest.json",
        root / "models/example-default/manifest.json",
        sync_json_manifest(root / "models/example-default/manifest.json", stable),
    ))

    # 4. Echo handler (excluded from workspace, follows Stable runtime version).
    targets.append((
        "handlers/echo-handler/Cargo.toml",
        root / "handlers/echo-handler/Cargo.toml",
        sync_handler_cargo_toml(root / "handlers/echo-handler/Cargo.toml", stable),
    ))
    targets.append((
        "handlers/echo-handler/manifest.json",
        root / "handlers/echo-handler/manifest.json",
        sync_json_manifest(root / "handlers/echo-handler/manifest.json", stable),
    ))

    # 5. Test malicious handler (excluded from workspace).
    targets.append((
        "handlers/test-malicious-handler/Cargo.toml",
        root / "handlers/test-malicious-handler/Cargo.toml",
        sync_handler_cargo_toml(root / "handlers/test-malicious-handler/Cargo.toml", stable),
    ))

    # 6. Documentation — follows Stable version.
    targets.append((
        "ROADMAP.md",
        root / "ROADMAP.md",
        sync_roadmap(root / "ROADMAP.md", stable),
    ))
    targets.append((
        "SECURITY.md",
        root / "SECURITY.md",
        sync_security(root / "SECURITY.md", stable),
    ))

    # 7. CHANGELOG skeleton — follows Stable version.
    targets.append((
        "CHANGELOG.md (skeleton)",
        root / "CHANGELOG.md",
        sync_changelog(root / "CHANGELOG.md", stable),
    ))

    # Summary.
    print()
    changed = 0
    for label, _, count in targets:
        status = f"{count} update(s)" if count else "ok"
        print(f"  {label:50s} {status}")
        changed += count

    # 8. Regression guard: README installation examples must use
    # ``cargo add rill-ml`` and must not reintroduce a literal
    # ``rill-ml = "0.x"`` TOML dependency line that can drift from the
    # canonical workspace version.
    readme_violations = 0
    for readme in (root / "README.md", root / "README.en.md"):
        found = verify_readme_no_hardcoded_version(readme)
        if found:
            readme_violations += found
            print(
                f"  {readme.name:50s} {found} hardcoded rill-ml version line(s) — "
                "use `cargo add rill-ml` instead",
                file=sys.stderr,
            )
        else:
            print(f"  {readme.name:50s} ok")

    print(f"\nsync: {changed} field(s) updated for stable {stable}, preview {preview}")
    if readme_violations:
        print(
            f"sync: {readme_violations} README hardcoded version line(s) must be removed",
            file=sys.stderr,
        )
        return 2
    print("sync: remember to fill in CHANGELOG.md release notes before releasing.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
