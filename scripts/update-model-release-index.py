#!/usr/bin/env python3
"""Replace only one model artifact in an already verified stable index."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--signed-index", type=Path, required=True)
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--model-id", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--url", required=True)
    parser.add_argument("--publisher-key-id", required=True)
    parser.add_argument("--generated-at", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    envelope = json.loads(args.signed_index.read_text(encoding="utf-8"))
    payload = envelope["payload"]
    if payload["publisherKeyId"] != args.publisher_key_id:
        raise SystemExit("existing index publisher does not match")
    runtimes = [item for item in payload["artifacts"] if item["kind"] == "runtime"]
    if not runtimes:
        raise SystemExit("stable index has no runtime; publish the bootstrap runtime release first")
    retained = [
        item
        for item in payload["artifacts"]
        if not (item["kind"] == "model" and item["id"] == args.model_id)
    ]
    previous = next(
        (
            item
            for item in payload["artifacts"]
            if item["kind"] == "model" and item["id"] == args.model_id
        ),
        None,
    )
    if previous and semver_key(args.version) <= semver_key(previous["version"]):
        raise SystemExit(
            f"model-only release must increase version beyond {previous['version']}"
        )
    content = args.model.read_bytes()
    retained.append(
        {
            "kind": "model",
            "id": args.model_id,
            "version": args.version,
            "runtimeApiVersion": 2,
            "url": args.url,
            "sha256": hashlib.sha256(content).hexdigest(),
            "size": len(content),
        }
    )
    next_payload = {
        "schemaVersion": 2,
        "channel": "stable",
        "generatedAt": args.generated_at,
        "publisherKeyId": args.publisher_key_id,
        "artifacts": retained,
    }
    args.output.write_text(
        json.dumps(next_payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def semver_key(version: str) -> tuple[tuple[int, int, int], int, str]:
    core, separator, prerelease = version.partition("-")
    parts = core.split(".")
    if len(parts) != 3 or not all(part.isdigit() for part in parts):
        raise SystemExit(f"unsupported semantic version: {version}")
    return (tuple(int(part) for part in parts), 1 if not separator else 0, prerelease)


if __name__ == "__main__":
    main()
