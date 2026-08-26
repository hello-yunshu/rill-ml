#!/usr/bin/env python3
"""Verify immutable release files against an already-verified signed index."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
from urllib.parse import urlparse



def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--index", type=Path, required=True)
    parser.add_argument("--release-dir", type=Path, required=True)
    parser.add_argument("--version", required=True)
    args = parser.parse_args()

    envelope = json.loads(args.index.read_text(encoding="utf-8"))
    expected = {
        PurePosixPath(urlparse(item["url"]).path).name: item
        for item in envelope["payload"]["artifacts"]
        if item["version"] == args.version
    }
    local = {
        path.name: path
        for pattern in ("rill-runtime-*", "*.rillpack", "*.rillhandler")
        for path in args.release_dir.glob(pattern)
        if path.is_file()
    }
    missing = sorted(set(expected) - set(local))
    # The current model can be absent from this version's index when an older
    # model supersedes it; it is still verified separately by rill-pack. No
    # Runtime binary is allowed to bypass the signed index, including
    # Core-only investigation builds. The macOS aarch64 unsigned sidecar is
    # publication metadata for the runtime asset, not a signed-index artifact.
    allowed_unindexed = {
        f"example-default-{args.version}.rillpack",
        f"rill-runtime-{args.version}-macos-aarch64.unsigned.json",
    }
    unexpected = sorted(
        name for name in set(local) - set(expected) if name not in allowed_unindexed
    )
    if not expected or missing or unexpected:
        raise SystemExit(
            f"release asset set differs from signed index; "
            f"missing={missing}, unexpected={unexpected}"
        )

    for name, path in local.items():
        item = expected.get(name)
        if item is None:
            # The version's example model can be superseded in the stable index;
            # rill-pack verifies its own signature separately before publication.
            continue
        content = path.read_bytes()
        digest = hashlib.sha256(content).hexdigest()
        if len(content) != item["size"] or digest != item["sha256"]:
            raise SystemExit(f"{name} differs from the signed immutable asset")


if __name__ == "__main__":
    main()
