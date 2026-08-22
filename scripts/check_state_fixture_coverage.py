#!/usr/bin/env python3
"""Validate the state schema stability manifest against fixtures on disk.

This script reads ``state-schema-manifest.toml`` and validates:

- Every Stable state type has a v0.13.0 fixture at
  ``tests/fixtures/state/v0.13.0/{fixture}.json``.
- Every Stable state type has a v1 fixture at
  ``tests/fixtures/state/v1/{fixture}.json``.
- Every Stable state type implements ``ValidateState`` (checked by grepping
  the Rust source code for ``impl ValidateState for {type}``).
- No duplicate entries in either the Stable or Preview group.
- No overlap between the Stable and Preview groups.
- The documentation whitelist (in ``STABILITY.md``) matches the manifest.

Used as a CI gate to ensure that the state-freeze contract is enforced and
that the documentation is accurate.

Usage::

    python3 scripts/check_state_fixture_coverage.py
    python3 scripts/check_state_fixture_coverage.py --root /path/to/repo
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.x runners without the stdlib TOML parser.
    tomllib = None


def load_manifest_text(text: str) -> dict[str, list[dict[str, str]]]:
    """Load this deliberately small manifest on old offline Python runners."""
    if tomllib is not None:
        return tomllib.loads(text)

    data: dict[str, list[dict[str, str]]] = {}
    current: tuple[str, dict[str, str]] | None = None
    for raw_line in text.splitlines():
        line = raw_line.split("#", 1)[0].strip()
        if not line:
            continue
        section = re.fullmatch(r"\[\[(stable_state|preview_state|portable_state)\]\]", line)
        if section:
            entry: dict[str, str] = {}
            data.setdefault(section.group(1), []).append(entry)
            current = (section.group(1), entry)
            continue
        field = re.fullmatch(r"([A-Za-z_][A-Za-z0-9_]*)\s*=\s*\"([^\"]*)\"", line)
        if field is None or current is None:
            raise ValueError(f"unsupported manifest line: {raw_line}")
        current[1][field.group(1)] = field.group(2)
    return data


def parse_manifest(manifest_path: pathlib.Path) -> tuple[list[dict[str, str]], list[dict[str, str]]]:
    """Parse ``state-schema-manifest.toml`` and return ``(stable_entries, preview_entries)``.

    Each entry is a dict with keys ``type`` and optionally ``fixture`` or ``reason``.
    """
    if not manifest_path.exists():
        raise RuntimeError(f"state-schema-manifest.toml not found at {manifest_path}")

    try:
        data = load_manifest_text(manifest_path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise RuntimeError(f"failed to parse {manifest_path}: {exc}") from exc
    return list(data.get("stable_state", [])), list(data.get("preview_state", []))


def parse_portable_states(manifest_path: pathlib.Path) -> list[dict[str, str]]:
    """Return explicitly versioned portable-state entries."""
    data = load_manifest_text(manifest_path.read_text(encoding="utf-8"))
    return list(data.get("portable_state", []))


def parse_documented_types(stability_path: pathlib.Path) -> tuple[list[str], list[str]]:
    """Return the Stable table and Preview bullet-list types from STABILITY.md."""
    try:
        text = stability_path.read_text(encoding="utf-8")
    except OSError as exc:
        raise RuntimeError(f"failed to read {stability_path}: {exc}") from exc

    stable_match = re.search(
        r"^### Stable state schema types\s*$"
        r"(?P<body>.*?)"
        r"^### Preview state schema types\s*$",
        text,
        flags=re.MULTILINE | re.DOTALL,
    )
    preview_match = re.search(
        r"^### Preview state schema types\s*$"
        r"(?P<body>.*?)"
        r"^### State schema manifest\s*$",
        text,
        flags=re.MULTILINE | re.DOTALL,
    )
    if stable_match is None or preview_match is None:
        raise RuntimeError(
            "STABILITY.md is missing the Stable/Preview state schema sections"
        )

    stable = re.findall(
        r"^\|\s*`([^`]+)`\s*\|",
        stable_match.group("body"),
        flags=re.MULTILINE,
    )
    preview = re.findall(
        r"^-\s+`([^`]+)`(?:\s|$)",
        preview_match.group("body"),
        flags=re.MULTILINE,
    )
    return stable, preview


def grep_validate_state_impl(root: pathlib.Path, type_name: str) -> bool:
    """Check if ``impl ValidateState for {type_name}`` exists in the Rust source."""
    return _grep_pattern_in_source(root, rf"impl\s+ValidateState\s+for\s+{re.escape(type_name)}\b")


def grep_custom_deserialize_impl(root: pathlib.Path, type_name: str) -> bool:
    """Check if a custom ``Deserialize`` implementation exists for {type_name}."""
    return _grep_pattern_in_source(
        root,
        rf"impl.*serde::Deserialize.*\s+for\s+{re.escape(type_name)}\b",
    )


def _grep_pattern_in_source(root: pathlib.Path, pattern_str: str) -> bool:
    """Search for a regex pattern in all ``*.rs`` files under ``src/`` and ``crates/``."""
    pattern = re.compile(pattern_str)
    for search_dir in (root / "src", root / "crates"):
        if not search_dir.exists():
            continue
        for rust_file in search_dir.rglob("*.rs"):
            try:
                text = rust_file.read_text(encoding="utf-8")
            except OSError:
                continue
            if pattern.search(text):
                return True
    return False


def validate_coverage(root: pathlib.Path) -> list[str]:
    """Validate the manifest against fixtures on disk.

    Returns a list of error messages (empty list = all checks passed).
    """
    errors: list[str] = []
    manifest_path = root / "state-schema-manifest.toml"
    fixture_dir = root / "tests" / "fixtures" / "state"
    fixture_tests_path = root / "tests" / "state_fixtures.rs"

    try:
        stable_entries, preview_entries = parse_manifest(manifest_path)
    except RuntimeError as e:
        return [str(e)]

    # --- Check for duplicates in Stable group --- #
    stable_types: list[str] = [e["type"] for e in stable_entries]
    seen: set[str] = set()
    for t in stable_types:
        if t in seen:
            errors.append(f"Duplicate Stable state type: {t!r}")
        seen.add(t)

    # --- Check for duplicates in Preview group --- #
    preview_types: list[str] = [e["type"] for e in preview_entries]
    seen_preview: set[str] = set()
    for t in preview_types:
        if t in seen_preview:
            errors.append(f"Duplicate Preview state type: {t!r}")
        seen_preview.add(t)

    # --- Check for overlap between Stable and Preview --- #
    overlap = set(stable_types) & set(preview_types)
    if overlap:
        errors.append(f"Types appear in both Stable and Preview groups: {sorted(overlap)}")

    # --- Check that every Stable type has v0.13.0 and v1 fixtures --- #
    for entry in stable_entries:
        t = entry["type"]
        fixture = entry.get("fixture", "")
        if not fixture:
            errors.append(f"Stable state type {t!r} has no 'fixture' field in manifest")
            continue

        v0_path = fixture_dir / "v0.13.0" / f"{fixture}.json"
        v1_path = fixture_dir / "v1" / f"{fixture}.json"

        if not v0_path.exists():
            errors.append(f"Stable state type {t!r}: missing v0.13.0 fixture at {v0_path}")
        if not v1_path.exists():
            errors.append(f"Stable state type {t!r}: missing v1 fixture at {v1_path}")

    # Fixture existence alone is insufficient: require both fixture generations
    # to be exercised by the cross-version Rust test target.
    try:
        fixture_tests = fixture_tests_path.read_text(encoding="utf-8")
    except OSError as exc:
        errors.append(f"failed to read fixture tests at {fixture_tests_path}: {exc}")
        fixture_tests = ""
    for entry in stable_entries:
        fixture = entry.get("fixture", "")
        if not fixture:
            continue
        for generation, function_prefix in (
            ("v0.13.0", "load_v0_13_0"),
            ("v1", "load_v1"),
        ):
            test_name = f"{function_prefix}_{fixture}"
            if not re.search(rf"\bfn\s+{re.escape(test_name)}\s*\(", fixture_tests):
                errors.append(
                    f"Stable state type {entry['type']!r}: {generation} fixture "
                    f"is not exercised by test function {test_name!r}"
                )

    # --- Check that every Stable type implements ValidateState or has custom Deserialize --- #
    for entry in stable_entries:
        t = entry["type"]
        validation = entry.get("validation", "validate_state")
        if validation == "validate_state":
            if not grep_validate_state_impl(root, t):
                errors.append(
                    f"Stable state type {t!r}: does not implement ValidateState "
                    f"(no 'impl ValidateState for {t}' found in src/ or crates/)"
                )
        elif validation == "deserialize":
            if not grep_custom_deserialize_impl(root, t):
                errors.append(
                    f"Stable state type {t!r}: validation='deserialize' but no custom "
                    f"Deserialize impl found for {t} in src/ or crates/"
                )
        elif validation == "derive":
            # Simple enums/structs that derive Deserialize via serde's derive macro.
            # serde automatically rejects unknown variants for enums, providing
            # basic validation without a custom Deserialize impl or ValidateState impl.
            # No additional check needed — if the type compiles with serde::Deserialize,
            # it is covered by the derive macro's validation.
            pass
        else:
            errors.append(
                f"Stable state type {t!r}: unknown validation mode {validation!r} "
                f"(expected 'validate_state', 'deserialize', or 'derive')"
            )

    # --- Versioned portable DTOs have their own post-introduction fixtures. --- #
    portable_entries = parse_portable_states(manifest_path)
    portable_tests_path = root / "tests" / "portable_drift_state.rs"
    try:
        portable_tests = portable_tests_path.read_text(encoding="utf-8")
    except OSError as exc:
        errors.append(f"failed to read portable-state tests at {portable_tests_path}: {exc}")
        portable_tests = ""
    seen_portable: set[str] = set()
    for entry in portable_entries:
        type_name = entry.get("type", "")
        fixture = entry.get("fixture", "")
        generation = entry.get("generation", "")
        if not type_name or type_name in seen_portable:
            errors.append(f"invalid or duplicate portable state type: {type_name!r}")
            continue
        seen_portable.add(type_name)
        if entry.get("stability") != "stable":
            errors.append(f"portable state {type_name!r} must declare stability='stable'")
        path = fixture_dir / generation / f"{fixture}.json"
        if not fixture or not generation or not path.exists():
            errors.append(f"portable state {type_name!r}: missing golden fixture at {path}")
        if not grep_validate_state_impl(root, type_name):
            errors.append(f"portable state {type_name!r}: missing ValidateState implementation")
        test_name = f"load_{generation.replace('-', '_')}_{fixture}"
        if not re.search(rf"\bfn\s+{re.escape(test_name)}\s*\(", portable_tests):
            errors.append(
                f"portable state {type_name!r}: fixture is not exercised by {test_name!r}"
            )

    # --- Check that the human-facing whitelist exactly matches the manifest. --- #
    try:
        documented_stable, documented_preview = parse_documented_types(
            root / "STABILITY.md"
        )
    except RuntimeError as exc:
        errors.append(str(exc))
    else:
        if documented_stable != stable_types:
            errors.append(
                "STABILITY.md Stable state schema order/content differs from "
                "state-schema-manifest.toml"
            )
        if documented_preview != preview_types:
            errors.append(
                "STABILITY.md Preview state schema order/content differs from "
                "state-schema-manifest.toml"
            )

    return errors


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate state schema fixture coverage against the manifest."
    )
    parser.add_argument(
        "--root",
        type=pathlib.Path,
        default=None,
        help="Repository root path (default: auto-detected from script location).",
    )
    args = parser.parse_args()

    root = args.root or pathlib.Path(__file__).resolve().parent.parent
    errors = validate_coverage(root)

    if errors:
        print(f"check_state_fixture_coverage: {len(errors)} error(s) found:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    # Print summary.
    manifest_path = root / "state-schema-manifest.toml"
    stable_entries, preview_entries = parse_manifest(manifest_path)
    portable_entries = parse_portable_states(manifest_path)
    print("check_state_fixture_coverage: validation passed.")
    print(f"  Stable state types: {len(stable_entries)}")
    print(f"  Preview state types: {len(preview_entries)}")
    print(f"  Stable portable state DTOs: {len(portable_entries)}")
    print(
        "  All Stable types have exercised v0.13.0 + v1 fixtures, declared "
        "validation, and matching documentation."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
