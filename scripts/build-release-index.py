#!/usr/bin/env python3
"""Build the unsigned, deterministic local-AI release-index payload."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


RUNTIMES = (
    ("linux", "x86_64", "rill-runtime-{version}-linux-x86_64"),
    ("macos", "x86_64", "rill-runtime-{version}-macos-x86_64"),
    ("macos", "aarch64", "rill-runtime-{version}-macos-aarch64"),
    ("windows", "x86_64", "rill-runtime-{version}-windows-x86_64.exe"),
)


def artifact(path: Path, **fields: object) -> dict[str, object]:
    content = path.read_bytes()
    return {
        **fields,
        "sha256": hashlib.sha256(content).hexdigest(),
        "size": len(content),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--release-dir", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--publisher-key-id", required=True)
    parser.add_argument("--generated-at", required=True)
    parser.add_argument("--existing-index", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    base_url = f"https://github.com/{args.repository}/releases/download/{args.tag}"
    artifacts: list[dict[str, object]] = []
    for target_os, target_arch, pattern in RUNTIMES:
        name = pattern.format(version=args.version)
        artifacts.append(
            artifact(
                args.release_dir / name,
                kind="runtime",
                id="rill-runtime",
                version=args.version,
                runtimeApiVersion=1,
                targetOs=target_os,
                targetArch=target_arch,
                url=f"{base_url}/{name}",
            )
        )

    existing_model = None
    if args.existing_index and args.existing_index.is_file():
        envelope = json.loads(args.existing_index.read_text(encoding="utf-8"))
        existing_model = next(
            (
                item
                for item in envelope["payload"]["artifacts"]
                if item["kind"] == "model" and item["id"] == "rillml.example.default"
            ),
            None,
        )
    if existing_model and semver_key(existing_model["version"]) > semver_key(args.version):
        artifacts.append(existing_model)
    else:
        model_name = f"example-default-{args.version}.rillpack"
        artifacts.append(
            artifact(
                args.release_dir / model_name,
                kind="model",
                id="rillml.example.default",
                version=args.version,
                runtimeApiVersion=1,
                url=f"{base_url}/{model_name}",
            )
        )
    payload = {
        "schemaVersion": 1,
        "channel": "stable",
        "generatedAt": args.generated_at,
        "publisherKeyId": args.publisher_key_id,
        "artifacts": artifacts,
    }
    args.output.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
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
