#!/usr/bin/env python3
"""Verify the ticker test probe is not part of rill-runtime's public API.

The existing rustdoc grep check (``.github/workflows/pipeline.yml``)
only greps the rendered rustdoc for ``active_epoch_ticker_count`` /
``ACTIVE_EPOCH_TICKERS``.  Because ``#[doc(hidden)]`` items are not
rendered by rustdoc, that check cannot detect a regression where the
probe is re-exposed as ``#[doc(hidden)] pub fn active_epoch_ticker_count()``.

This script takes the stricter "external crate" view: it creates a
temporary crate that depends on ``rill-runtime`` (via path) and attempts
to ``use rill_runtime::handler::wasm::active_epoch_ticker_count;``.  The
check passes only if that crate **fails to compile**.  A companion smoke
crate proves that legitimate public API
(``rill_runtime::handler::wasm::WasmInvokeHandler``) still compiles, so a
broken dependency configuration cannot be misread as "probe invisible".

Exit codes:
  0  probe is not externally accessible (expected)
  1  probe compiled successfully (public API leak)
  2  smoke crate failed to compile (dependency misconfiguration)
  3  internal error (could not run cargo, write temp files, etc.)
"""

from __future__ import annotations

import os
import pathlib
import subprocess
import sys
import tempfile
from typing import Tuple

# Markers that cargo/rustc emits when an import path is not externally
# reachable.  The exact wording varies by failure mode:
#   - ``unresolved import`` — the path component does not exist
#   - ``cannot find`` — similar, used by some rustc versions
#   - ``private`` — the item exists but is not ``pub``
# Any one of these is sufficient evidence that the probe is not part of
# the public API.
REJECTION_MARKERS: Tuple[str, ...] = (
    "unresolved import",
    "cannot find",
    "private",
)

# Markers that indicate ``cargo check --offline`` failed because a needed
# crate was not in the local cache.  In that case we retry online so a
# cold cache does not produce a false positive.
OFFLINE_RETRY_MARKERS: Tuple[str, ...] = (
    "can't find crate",
    "no matching package",
    "failed to download",
)


def workspace_root() -> pathlib.Path:
    """Return the repository root (parent of the ``scripts/`` directory)."""
    return pathlib.Path(__file__).resolve().parents[1]


def runtime_crate_path(root: pathlib.Path) -> pathlib.Path:
    """Return the path to the ``rill-runtime`` crate directory."""
    return root / "crates" / "rill-runtime"


SMOKE_MAIN_RS = """\
// Smoke crate: prove that legitimate public runtime API compiles when
// rill-runtime is consumed as an external dependency with --features wasm.
use rill_runtime::handler::wasm::WasmInvokeHandler;

fn main() {
    let _ = std::any::TypeId::of::<WasmInvokeHandler>();
}
"""

PROBE_MAIN_RS = """\
// Probe crate: attempt to import the test-only ticker probe.  This MUST
// fail to compile; if it compiles, the probe has leaked into the public
// API (including via `#[doc(hidden)] pub`, which rustdoc grep cannot
// detect).
use rill_runtime::handler::wasm::active_epoch_ticker_count;

fn main() {
    let _ = active_epoch_ticker_count();
}
"""


def _cargo_toml(crate_name: str, runtime_path: pathlib.Path) -> str:
    """Render a minimal Cargo.toml that depends on rill-runtime via path."""
    return (
        "[package]\n"
        f'name = "{crate_name}"\n'
        'version = "0.0.0"\n'
        'edition = "2024"\n'
        'rust-version = "1.94"\n'
        'publish = false\n'
        "\n"
        "[dependencies]\n"
        f'rill-runtime = {{ path = "{runtime_path}", features = ["wasm"] }}\n'
    )


def write_crate(
    parent: pathlib.Path, crate_name: str, main_rs: str, runtime_path: pathlib.Path
) -> pathlib.Path:
    """Write a throwaway crate under ``parent/<crate_name>/`` and return its path."""
    crate_dir = parent / crate_name
    crate_dir.mkdir(parents=True, exist_ok=True)
    src_dir = crate_dir / "src"
    src_dir.mkdir(exist_ok=True)
    (crate_dir / "Cargo.toml").write_text(
        _cargo_toml(crate_name, runtime_path), encoding="utf-8"
    )
    (src_dir / "main.rs").write_text(main_rs, encoding="utf-8")
    return crate_dir


def run_cargo_check(crate_dir: pathlib.Path) -> Tuple[int, str, str]:
    """Run ``cargo check`` in ``crate_dir`` and return (returncode, stdout, stderr).

    Prefers ``--offline`` so the check does not hit the network when the
    dependency cache is warm.  If offline fails because a dep is missing
    locally, retries online so a cold cache does not produce a false
    positive.
    """
    env = os.environ.copy()
    manifest = str(crate_dir / "Cargo.toml")
    offline = subprocess.run(
        ["cargo", "check", "--offline", "--manifest-path", manifest],
        capture_output=True,
        text=True,
        env=env,
        check=False,
    )
    if offline.returncode != 0 and any(
        marker in offline.stderr for marker in OFFLINE_RETRY_MARKERS
    ):
        online = subprocess.run(
            ["cargo", "check", "--manifest-path", manifest],
            capture_output=True,
            text=True,
            env=env,
            check=False,
        )
        return online.returncode, online.stdout, online.stderr
    return offline.returncode, offline.stdout, offline.stderr


def classify_probe_result(returncode: int, stderr: str) -> str:
    """Classify the probe crate's cargo check result.

    Returns ``"rejected"`` if the probe is not publicly accessible
    (expected) or ``"leaked"`` if the probe compiled successfully
    (public API leak).
    """
    if returncode == 0:
        return "leaked"
    return "rejected"


def is_rejection(returncode: int, stderr: str) -> bool:
    """Return True if the probe crate was correctly rejected with a known marker."""
    if returncode == 0:
        return False
    return any(marker in stderr for marker in REJECTION_MARKERS)


def verify_smoke_succeeds(returncode: int, stderr: str) -> bool:
    """Return True if the smoke crate compiled successfully."""
    return returncode == 0


def main() -> int:
    root = workspace_root()
    runtime_path = runtime_crate_path(root)
    if not (runtime_path / "Cargo.toml").is_file():
        print(
            f"error: rill-runtime crate not found at {runtime_path}", file=sys.stderr
        )
        return 3

    try:
        with tempfile.TemporaryDirectory(prefix="rill-runtime-public-api-") as temp_name:
            temp = pathlib.Path(temp_name)

            # 1. Smoke crate: legitimate public API must compile.
            smoke_dir = write_crate(
                temp, "smoke_normal_api", SMOKE_MAIN_RS, runtime_path
            )
            smoke_rc, smoke_out, smoke_err = run_cargo_check(smoke_dir)
            if not verify_smoke_succeeds(smoke_rc, smoke_err):
                print(
                    "error: normal public API smoke crate failed to compile; "
                    "dependency configuration is broken",
                    file=sys.stderr,
                )
                print("--- smoke crate stderr ---", file=sys.stderr)
                print(smoke_err, file=sys.stderr)
                return 2
            print("normal public API smoke crate: PASS")

            # 2. Probe crate: ticker probe import must NOT compile.
            probe_dir = write_crate(
                temp, "probe_ticker_import", PROBE_MAIN_RS, runtime_path
            )
            probe_rc, probe_out, probe_err = run_cargo_check(probe_dir)
            if probe_rc == 0:
                print(
                    "error: ticker probe compiled successfully — public API leak",
                    file=sys.stderr,
                )
                print("--- probe crate stdout ---", file=sys.stderr)
                print(probe_out, file=sys.stderr)
                return 1
            if not is_rejection(probe_rc, probe_err):
                print(
                    "error: ticker probe was rejected, but stderr did not contain "
                    f"any of {REJECTION_MARKERS}",
                    file=sys.stderr,
                )
                print("--- probe crate stderr ---", file=sys.stderr)
                print(probe_err, file=sys.stderr)
                return 1
            print("ticker probe external import: rejected as expected")
    except FileNotFoundError as exc:
        print(f"error: required tool not found: {exc}", file=sys.stderr)
        return 3
    except OSError as exc:
        print(f"error: filesystem error: {exc}", file=sys.stderr)
        return 3

    return 0


if __name__ == "__main__":
    sys.exit(main())
