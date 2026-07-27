#!/usr/bin/env python3
"""Verify the frozen WIT ABI v1 surface is internally consistent.

The 1.0 freeze pins four independent declarations of the handler ABI version:

1. ``crates/rill-handler-api/wit/rill-handler.wit`` — the canonical WIT
   source. The ``package rill:handler@<version>;`` line is parsed.
2. ``crates/rill-handler-api/src/lib.rs`` — the Rust constants
   ``HANDLER_API_VERSION``, ``WIT_PACKAGE``, ``WIT_VERSION`` and
   ``WIT_WORLD`` that the host and guest use for compile-time checks.
3. ``crates/rill-runtime-protocol/src/lib.rs`` — the protocol constant
   ``HANDLER_API_VERSION`` embedded in handler-pack manifest validation.
4. ``crates/rill-handler-api/wit/rill-handler.wit`` SHA-256 — a hash of the
   normalised WIT text so any structural edit is detected even if the
   package version line is unchanged.

This script asserts that all four agree, and that the normalised WIT hash
matches the frozen hash recorded in this script. The hash is intentionally
inlined so a silent edit to the WIT file is caught even when the version
strings are unchanged.

Run from the repository root:

    python3 scripts/check_wit_abi.py

Exit codes:
    0  all WIT ABI declarations agree and the hash matches the frozen value
    1  mismatch detected (message on stderr)
    2  internal error (missing file, parse failure, etc.)
"""

from __future__ import annotations

import hashlib
import pathlib
import re
import sys
from typing import Optional

# Frozen WIT ABI v1 declarations. Updating any of these values is a
# breaking change that requires a new ``rill:handler@2.x`` package and a
# new ``HANDLER_API_VERSION``.
FROZEN_HANDLER_API_VERSION = 1
FROZEN_WIT_PACKAGE = "rill:handler"
FROZEN_WIT_VERSION = "1.0.0"
FROZEN_WIT_WORLD = "invoke-handler"

# SHA-256 of the normalised WIT text (see ``normalise_wit``). The normalised
# form strips trailing whitespace, collapses consecutive blank lines, and
# enforces a single trailing newline so cosmetic edits do not change the
# hash. Any semantic change to the WIT (new function, new record field,
# renamed variant, etc.) will change this hash and fail the check.
#
# Frozen on 2026-07-28 from commit d77ef98b (work/1.0-freeze) for the
# 1.0.0-rc.1 release. The WIT source has been pin-verified against the
# published rill:handler@1.0.0 ABI; any change here requires a new
# rill:handler@2.x package and a HANDLER_API_VERSION bump.
FROZEN_WIT_SHA256 = "108a68dfd6bcf86e3b63ad630508b2bbf407d00e8634067366e53dbc257cc90c"


def workspace_root() -> pathlib.Path:
    return pathlib.Path(__file__).resolve().parents[1]


def wit_path(root: pathlib.Path) -> pathlib.Path:
    return root / "crates" / "rill-handler-api" / "wit" / "rill-handler.wit"


def runtime_wit_copy_path(root: pathlib.Path) -> pathlib.Path:
    """Path to the WIT copy bundled inside ``rill-runtime`` for packaging.

    ``cargo package`` creates a self-contained tarball that cannot resolve
    the ``../rill-handler-api/wit/...`` relative path used in development.
    A verbatim copy lives at ``crates/rill-runtime/wit/rill-handler.wit``
    so the ``bindgen!`` macro can find it via ``wit/rill-handler.wit``.
    This check ensures the copy stays byte-identical to the canonical source.
    """
    return root / "crates" / "rill-runtime" / "wit" / "rill-handler.wit"


def handler_api_lib_path(root: pathlib.Path) -> pathlib.Path:
    return root / "crates" / "rill-handler-api" / "src" / "lib.rs"


def runtime_protocol_lib_path(root: pathlib.Path) -> pathlib.Path:
    return root / "crates" / "rill-runtime-protocol" / "src" / "lib.rs"


def normalise_wit(text: str) -> str:
    """Return a canonical form of the WIT text for hashing.

    The normalisation is deliberately conservative: it only strips
    presentation-level differences (trailing whitespace per line, leading
    and trailing blank lines, consecutive blank lines, line endings). Any
    semantic edit (whitespace inside a line, new declarations, renamed
    identifiers) will change the hash.
    """
    lines = [line.rstrip() for line in text.splitlines()]
    # Collapse consecutive blank lines into a single blank line.
    collapsed: list[str] = []
    prev_blank = False
    for line in lines:
        is_blank = line == ""
        if is_blank and prev_blank:
            continue
        collapsed.append(line)
        prev_blank = is_blank
    # Strip leading and trailing blank lines.
    while collapsed and collapsed[0] == "":
        collapsed.pop(0)
    while collapsed and collapsed[-1] == "":
        collapsed.pop()
    return "\n".join(collapsed) + "\n"


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def parse_wit_package_version(text: str) -> Optional[tuple[str, str]]:
    """Extract (package, version) from ``package rill:handler@1.0.0;``."""
    match = re.search(r"^package\s+([\w:-]+)@([\d.]+)\s*;", text, re.MULTILINE)
    if match:
        return match.group(1), match.group(2)
    return None


def parse_wit_world(text: str) -> Optional[str]:
    """Extract the world name from ``world invoke-handler {``."""
    match = re.search(r"^world\s+([\w-]+)\s*\{", text, re.MULTILINE)
    if match:
        return match.group(1)
    return None


def parse_rust_const(text: str, name: str) -> Optional[str]:
    """Extract the value of a ``pub const NAME: T = "value";`` declaration."""
    # Match pub const HANDLER_API_VERSION: u32 = 1;
    # Match pub const WIT_PACKAGE: &str = "rill:handler";
    # The type token may be a single word (u32) or a reference type (&str).
    pattern = rf'pub\s+const\s+{re.escape(name)}\s*:\s*&?[\w]+\s*=\s*("([^"]*)"|(\d+))\s*;'
    match = re.search(pattern, text)
    if match:
        if match.group(2) is not None:
            return match.group(2)
        return match.group(3)
    return None


def fail(message: str) -> int:
    print(f"error: {message}", file=sys.stderr)
    return 1


def main() -> int:
    root = workspace_root()

    wit_file = wit_path(root)
    if not wit_file.is_file():
        return fail(f"WIT file not found: {wit_file}")
    wit_text = wit_file.read_text(encoding="utf-8")

    handler_api_lib = handler_api_lib_path(root)
    if not handler_api_lib.is_file():
        return fail(f"rill-handler-api lib.rs not found: {handler_api_lib}")
    handler_api_text = handler_api_lib.read_text(encoding="utf-8")

    runtime_protocol_lib = runtime_protocol_lib_path(root)
    if not runtime_protocol_lib.is_file():
        return fail(f"rill-runtime-protocol lib.rs not found: {runtime_protocol_lib}")
    runtime_protocol_text = runtime_protocol_lib.read_text(encoding="utf-8")

    # 1. WIT package version.
    pkg = parse_wit_package_version(wit_text)
    if pkg is None:
        return fail("could not parse 'package <name>@<version>;' from WIT")
    wit_package, wit_version = pkg
    if wit_package != FROZEN_WIT_PACKAGE:
        return fail(
            f"WIT package '{wit_package}' != frozen '{FROZEN_WIT_PACKAGE}'"
        )
    if wit_version != FROZEN_WIT_VERSION:
        return fail(
            f"WIT version '{wit_version}' != frozen '{FROZEN_WIT_VERSION}'"
        )

    # 2. WIT world name.
    wit_world = parse_wit_world(wit_text)
    if wit_world is None:
        return fail("could not parse 'world <name> {' from WIT")
    if wit_world != FROZEN_WIT_WORLD:
        return fail(
            f"WIT world '{wit_world}' != frozen '{FROZEN_WIT_WORLD}'"
        )

    # 3. Normalised WIT hash.
    normalised = normalise_wit(wit_text)
    actual_hash = sha256_hex(normalised.encode("utf-8"))
    if actual_hash != FROZEN_WIT_SHA256:
        return fail(
            f"normalised WIT SHA-256 mismatch: expected {FROZEN_WIT_SHA256}, "
            f"got {actual_hash}. The WIT text was edited; either revert the "
            f"edit or, for an intentional additive change within v1, update "
            f"FROZEN_WIT_SHA256 in this script after review."
        )

    # 3b. Runtime WIT copy must be byte-identical to the canonical source.
    runtime_wit_copy = runtime_wit_copy_path(root)
    if not runtime_wit_copy.is_file():
        return fail(
            f"rill-runtime WIT copy not found: {runtime_wit_copy}. "
            f"Run: cp crates/rill-handler-api/wit/rill-handler.wit "
            f"crates/rill-runtime/wit/"
        )
    copy_text = runtime_wit_copy.read_text(encoding="utf-8")
    if copy_text != wit_text:
        return fail(
            f"rill-runtime WIT copy differs from canonical source. "
            f"Run: cp crates/rill-handler-api/wit/rill-handler.wit "
            f"crates/rill-runtime/wit/"
        )

    # 4. rill-handler-api Rust constants.
    rust_api_version = parse_rust_const(handler_api_text, "HANDLER_API_VERSION")
    rust_wit_package = parse_rust_const(handler_api_text, "WIT_PACKAGE")
    rust_wit_version = parse_rust_const(handler_api_text, "WIT_VERSION")
    rust_wit_world = parse_rust_const(handler_api_text, "WIT_WORLD")
    if rust_api_version != str(FROZEN_HANDLER_API_VERSION):
        return fail(
            f"rill-handler-api HANDLER_API_VERSION={rust_api_version} != {FROZEN_HANDLER_API_VERSION}"
        )
    if rust_wit_package != FROZEN_WIT_PACKAGE:
        return fail(
            f"rill-handler-api WIT_PACKAGE={rust_wit_package!r} != {FROZEN_WIT_PACKAGE!r}"
        )
    if rust_wit_version != FROZEN_WIT_VERSION:
        return fail(
            f"rill-handler-api WIT_VERSION={rust_wit_version!r} != {FROZEN_WIT_VERSION!r}"
        )
    if rust_wit_world != FROZEN_WIT_WORLD:
        return fail(
            f"rill-handler-api WIT_WORLD={rust_wit_world!r} != {FROZEN_WIT_WORLD!r}"
        )

    # 5. rill-runtime-protocol HANDLER_API_VERSION constant.
    proto_api_version = parse_rust_const(runtime_protocol_text, "HANDLER_API_VERSION")
    if proto_api_version != str(FROZEN_HANDLER_API_VERSION):
        return fail(
            f"rill-runtime-protocol HANDLER_API_VERSION={proto_api_version} != {FROZEN_HANDLER_API_VERSION}"
        )

    # All checks passed.
    print(f"WIT package: {wit_package}@{wit_version}")
    print(f"WIT world: {wit_world}")
    print(f"HANDLER_API_VERSION: {FROZEN_HANDLER_API_VERSION}")
    print(f"normalised WIT SHA-256: {actual_hash}")
    print("WIT ABI v1 freeze: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
