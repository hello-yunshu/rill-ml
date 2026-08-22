#!/usr/bin/env python3
"""Generate deterministic, dependency-complete CycloneDX and SPDX SBOMs."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import quote


ROOT = Path(__file__).resolve().parents[1]


def cargo_inventory() -> tuple[list[dict[str, object]], dict[str, list[str]]]:
    completed = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    document = json.loads(completed.stdout)
    packages = document["packages"]
    package_ids = {item["id"] for item in packages}
    inventory: list[dict[str, object]] = []
    for item in sorted(packages, key=lambda value: (value["name"], value["version"], value["id"])):
        inventory.append(
            {
                "id": item["id"],
                "name": item["name"],
                "version": item["version"],
                "source": item.get("source"),
                "checksum": item.get("checksum"),
                "purl": f"pkg:cargo/{quote(item['name'], safe='-_.')}@{item['version']}",
                "type": "library",
            }
        )
    dependencies: dict[str, list[str]] = {item["id"]: [] for item in inventory}
    resolve = document.get("resolve") or {}
    for node in resolve.get("nodes", []):
        dependencies[node["id"]] = sorted(
            {
                dependency["pkg"]
                for dependency in node.get("deps", [])
                if dependency.get("pkg") in package_ids
            }
        )
    return inventory, dependencies


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
                "sha1": hashlib.sha1(content).hexdigest(),
                "sha256": hashlib.sha256(content).hexdigest(),
                "size": len(content),
            }
        )
    return sorted(evidence, key=lambda item: str(item["name"]))


def release_timestamp(commit: str) -> str:
    raw_epoch = os.environ.get("SOURCE_DATE_EPOCH")
    if raw_epoch is not None:
        instant = datetime.fromtimestamp(int(raw_epoch), tz=timezone.utc)
    else:
        try:
            completed = subprocess.run(
                ["git", "show", "-s", "--format=%cI", commit],
                cwd=ROOT,
                check=True,
                capture_output=True,
                text=True,
            )
            instant = datetime.fromisoformat(completed.stdout.strip().replace("Z", "+00:00"))
            instant = instant.astimezone(timezone.utc)
        except (OSError, ValueError, subprocess.CalledProcessError):
            # Synthetic commits are used by offline tests. Keep their output
            # deterministic while release commits use their commit timestamp.
            instant = datetime.fromtimestamp(0, tz=timezone.utc)
    return instant.isoformat(timespec="seconds").replace("+00:00", "Z")


def write_json(path: Path, value: object) -> None:
    path.write_text(
        json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def spdx_download_location(source: str | None) -> str:
    """Return an SPDX-valid URL while retaining Cargo's exact source separately."""
    if source is None:
        return "NOASSERTION"
    for prefix in ("registry+", "sparse+", "git+"):
        if source.startswith(prefix):
            location = source[len(prefix) :]
            return location.split("#", 1)[0]
    return "NOASSERTION"


def spdx_artifact_ref(name: str) -> str:
    """Create an SPDX 2.3 identifier from an arbitrary release filename."""
    # SPDX identifiers permit only letters, numbers, ``.`` and ``-`` after
    # the required ``SPDXRef-`` prefix. Release asset names use underscores
    # for target/libc separation, so normalize every other character instead
    # of emitting an invalid document that only the in-repo checker accepts.
    normalized = re.sub(r"[^A-Za-z0-9.-]+", "-", name)
    return f"SPDXRef-Artifact-{normalized}"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--artifact", action="append", default=[])
    args = parser.parse_args()

    components, dependency_ids = cargo_inventory()
    artifacts = artifact_evidence(args.artifact)
    timestamp = release_timestamp(args.commit)
    identity = {
        "version": args.version,
        "tag": args.tag,
        "commit": args.commit,
        "artifacts": artifacts,
        "dependencyCount": len(components),
    }
    args.output_dir.mkdir(parents=True, exist_ok=True)
    bom_refs = {
        item["id"]: f"cargo:{hashlib.sha256(item['id'].encode()).hexdigest()[:16]}"
        for item in components
    }
    root_ref = f"application:rill-ml@{args.version}"
    serial_digest = hashlib.sha256(f"{args.tag}:{args.commit}".encode()).hexdigest()
    serial_number = (
        "urn:uuid:"
        f"{serial_digest[:8]}-{serial_digest[8:12]}-{serial_digest[12:16]}-"
        f"{serial_digest[16:20]}-{serial_digest[20:32]}"
    )

    cdx_components = []
    for item in components:
        component = {
            "bom-ref": bom_refs[item["id"]],
            "name": item["name"],
            "purl": item["purl"],
            "type": item["type"],
            "version": item["version"],
            "properties": [
                {"name": "rillml.release.version", "value": args.version},
                {"name": "rillml.release.tag", "value": args.tag},
                {"name": "rillml.release.commit", "value": args.commit},
            ],
        }
        if item.get("source"):
            component["properties"].append(
                {"name": "rillml.cargo.source", "value": item["source"]}
            )
        if item.get("checksum"):
            component["hashes"] = [{"alg": "SHA-256", "content": item["checksum"]}]
        cdx_components.append(component)
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
    cdx_dependencies = []
    for package_id in sorted(dependency_ids, key=lambda value: bom_refs[value]):
        dependency_entry = {"ref": bom_refs[package_id]}
        if dependency_ids[package_id]:
            dependency_entry["dependsOn"] = [
                bom_refs[dependency] for dependency in dependency_ids[package_id]
            ]
        cdx_dependencies.append(dependency_entry)
    workspace_ids = {item["id"] for item in components if item["source"] is None}
    cdx_dependencies.insert(
        0,
        {
            "ref": root_ref,
            "dependsOn": [bom_refs[item["id"]] for item in components if item["id"] in workspace_ids],
        },
    )
    cdx = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": serial_number,
        "version": 1,
        "metadata": {
            "timestamp": timestamp,
            "component": {
                "bom-ref": root_ref,
                "name": "rill-ml",
                "version": args.version,
                "type": "application",
            },
            "properties": [
                {"name": "rillml.release.tag", "value": args.tag},
                {"name": "rillml.release.commit", "value": args.commit},
            ],
        },
        "components": cdx_components,
        "dependencies": cdx_dependencies,
        "properties": [
            {"name": "rillml.release.identity", "value": json.dumps(identity, sort_keys=True, separators=(",", ":"))}
        ],
    }

    spdx_refs = {
        item["id"]: f"SPDXRef-Package-{hashlib.sha256(item['id'].encode()).hexdigest()[:16]}"
        for item in components
    }
    spdx_packages = []
    for item in components:
        package = {
            "SPDXID": spdx_refs[item["id"]],
            "name": item["name"],
            "versionInfo": item["version"],
            "downloadLocation": spdx_download_location(item.get("source")),
            "filesAnalyzed": False,
            "licenseConcluded": "NOASSERTION",
            "licenseDeclared": "NOASSERTION",
            "copyrightText": "NOASSERTION",
            "externalRefs": [
                {
                    "referenceCategory": "PACKAGE-MANAGER",
                    "referenceType": "purl",
                    "referenceLocator": item["purl"],
                }
            ],
        }
        if item.get("source"):
            package["sourceInfo"] = f"Cargo source: {item['source']}"
        if item.get("checksum"):
            package["checksums"] = [{"algorithm": "SHA256", "checksumValue": item["checksum"]}]
        spdx_packages.append(package)
    spdx_files = [
        {
            "SPDXID": spdx_artifact_ref(str(item["name"])),
            "fileName": item["name"],
            "checksums": [
                {
                    "algorithm": "SHA1",
                    "checksumValue": item["sha1"],
                },
                {"algorithm": "SHA256", "checksumValue": item["sha256"]},
            ],
            "licenseConcluded": "NOASSERTION",
            "licenseInfoInFiles": ["NOASSERTION"],
            "copyrightText": "NOASSERTION",
        }
        for item in artifacts
    ]
    root_package = next((item for item in components if item["name"] == "rill-ml"), components[0])
    relationships = [
        {"spdxElementId": "SPDXRef-DOCUMENT", "relationshipType": "DESCRIBES", "relatedSpdxElement": spdx_refs[root_package["id"]]},
    ]
    for package_id, dependency_list in dependency_ids.items():
        relationships.extend(
            {
                "spdxElementId": spdx_refs[package_id],
                "relationshipType": "DEPENDS_ON",
                "relatedSpdxElement": spdx_refs[dependency],
            }
            for dependency in dependency_list
        )
    relationships.extend(
        {
            "spdxElementId": spdx_refs[root_package["id"]],
            "relationshipType": "GENERATES",
            "relatedSpdxElement": file["SPDXID"],
        }
        for file in spdx_files
    )
    spdx = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"rill-ml-{args.version}",
        "documentNamespace": f"https://rillml.dev/sbom/{args.tag}/{args.commit}",
        "creationInfo": {
            "created": timestamp,
            "creators": ["Tool: rill-ml/scripts/generate_sbom.py"],
        },
        "packages": spdx_packages,
        "files": spdx_files,
        "relationships": relationships,
        "annotations": [
            {
                "annotationDate": timestamp,
                "annotationType": "OTHER",
                "annotator": "Tool: rill-ml/scripts/generate_sbom.py",
                "comment": json.dumps(identity, sort_keys=True, separators=(",", ":")),
            }
        ],
    }
    write_json(args.output_dir / f"rill-ml-{args.version}.cdx.json", cdx)
    write_json(args.output_dir / f"rill-ml-{args.version}.spdx.json", spdx)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
