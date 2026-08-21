#!/usr/bin/env python3
"""Generate deterministic CycloneDX JSON and SPDX JSON release SBOMs."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def metadata() -> list[dict[str, str]]:
    completed = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    packages = json.loads(completed.stdout)["packages"]
    return [
        {"name": item["name"], "version": item["version"], "type": "library"}
        for item in sorted(packages, key=lambda value: (value["name"], value["version"]))
    ]


def artifact_evidence(values: list[str]) -> list[dict[str, object]]:
    evidence = []
    for value in values:
        name, separator, raw_path = value.partition("=")
        if not separator or not name or not raw_path:
            raise SystemExit(f"--artifact must be NAME=PATH, got {value!r}")
        path = Path(raw_path)
        content = path.read_bytes()
        evidence.append(
            {
                "name": name,
                "path": name,
                "sha256": hashlib.sha256(content).hexdigest(),
                "size": len(content),
            }
        )
    return sorted(evidence, key=lambda item: str(item["name"]))


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--artifact", action="append", default=[])
    args = parser.parse_args()

    components = metadata()
    artifacts = artifact_evidence(args.artifact)
    identity = {
        "version": args.version,
        "tag": args.tag,
        "commit": args.commit,
        "artifacts": artifacts,
    }
    args.output_dir.mkdir(parents=True, exist_ok=True)

    cdx_components = [
        {
            "bom-ref": f"pkg:cargo/{item['name']}@{item['version']}",
            "name": item["name"],
            "purl": f"pkg:cargo/{item['name']}@{item['version']}",
            "type": item["type"],
            "version": item["version"],
            "properties": [
                {"name": "rillml.release.version", "value": args.version},
                {"name": "rillml.release.tag", "value": args.tag},
                {"name": "rillml.release.commit", "value": args.commit},
            ],
        }
        for item in components
    ]
    cdx_components.extend(
        {
            "bom-ref": f"artifact:{item['name']}",
            "name": item["name"],
            "type": "application",
            "version": args.version,
            "hashes": [{"alg": "SHA-256", "content": item["sha256"]}],
            "properties": [
                {"name": "rillml.release.tag", "value": args.tag},
                {"name": "rillml.release.commit", "value": args.commit},
                {"name": "rillml.artifact.size", "value": str(item["size"])},
            ],
        }
        for item in artifacts
    )
    cdx = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": f"urn:rillml:release:{args.tag}",
        "version": 1,
        "metadata": {
            "component": {"name": "rill-ml", "version": args.version, "type": "application"},
            "properties": [
                {"name": "rillml.release.tag", "value": args.tag},
                {"name": "rillml.release.commit", "value": args.commit},
            ],
        },
        "components": cdx_components,
        "properties": [{"name": "rillml.release.identity", "value": json.dumps(identity, sort_keys=True, separators=(",", ":"))}],
    }
    spdx_packages = [
        {
            "SPDXID": f"SPDXRef-Package-{item['name']}",
            "name": item["name"],
            "versionInfo": item["version"],
            "downloadLocation": "NOASSERTION",
            "filesAnalyzed": False,
        }
        for item in components
    ]
    spdx_files = [
        {
            "SPDXID": f"SPDXRef-Artifact-{item['name']}",
            "fileName": item["name"],
            "checksums": [{"algorithm": "SHA256", "checksumValue": item["sha256"]}],
        }
        for item in artifacts
    ]
    spdx = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"rill-ml-{args.version}",
        "documentNamespace": f"https://rillml.dev/sbom/{args.tag}/{args.commit}",
        "creationInfo": {"creators": ["Tool: rill-ml/scripts/generate_sbom.py"]},
        "packages": spdx_packages,
        "files": spdx_files,
        "annotations": [
            {"annotationType": "OTHER", "annotator": "Tool: rill-ml/scripts/generate_sbom.py", "comment": json.dumps(identity, sort_keys=True, separators=(",", ":"))}
        ],
    }
    write_json(args.output_dir / f"rill-ml-{args.version}.cdx.json", cdx)
    write_json(args.output_dir / f"rill-ml-{args.version}.spdx.json", spdx)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
