#!/usr/bin/env python3
"""Build the unsigned, deterministic local-AI release-index payload."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
from urllib.parse import urlparse


RUNTIMES = (
    ("linux", "x86_64", "rill-runtime-{version}-linux-x86_64"),
    ("macos", "aarch64", "rill-runtime-{version}-macos-aarch64"),
    ("windows", "x86_64", "rill-runtime-{version}-windows-x86_64.exe"),
    # Phase 4: native Windows ARM64 Runtime built on the `windows-11-arm`
    # hosted runner. Asset naming follows the existing stable
    # <os>-<arch> contract (targetOs=windows, targetArch=aarch64).
    ("windows", "aarch64", "rill-runtime-{version}-windows-aarch64.exe"),
)

RUNTIME_API_VERSION = 2
RELEASE_INDEX_SCHEMA_VERSION = 2
HANDLER_API_VERSION = 1

# Schemes that must never appear in a signed release-index URL. ``data:``,
# ``file:``, ``javascript:`` and similar schemes can be used to trick a
# downstream client into reading local files or executing inline payloads.
FORBIDDEN_URL_SCHEMES = frozenset({"file", "data", "javascript", "ftp", "blob"})

# Hostnames that resolve to the local machine. Production release indexes
# must never point at localhost because downstream clients on other hosts
# would fail to fetch the artifact (or worse, fetch a different artifact
# served by a local process). A test or development environment can opt in
# by setting ``RILL_ALLOW_LOCALHOST_URLS=1`` so that local fixture servers
# can be used without weakening the production policy.
LOCALHOST_HOSTS = frozenset({"localhost", "127.0.0.1", "::1", "0.0.0.0"})


def _localhost_allowed() -> bool:
    """Return True if the environment explicitly allows localhost URLs.

    Gated by the ``RILL_ALLOW_LOCALHOST_URLS`` environment variable so a
    production release pipeline (which must not set this variable) never
    silently accepts a localhost URL. Tests and local development set
    ``RILL_ALLOW_LOCALHOST_URLS=1`` to exercise the URL builder against a
    local fixture server.
    """
    return os.environ.get("RILL_ALLOW_LOCALHOST_URLS", "") == "1"


def validate_release_url(url: str) -> None:
    """Reject URLs that are not HTTPS with a non-empty host and no userinfo.

    The release index is signed and distributed to downstream clients that
    fetch artifacts based solely on the URL we record. A weak URL policy
    would let a compromised publisher point the index at ``file:///`` or
    ``http://`` endpoints. The policy is intentionally strict: HTTPS only,
    no embedded credentials, a non-empty host, no dangerous schemes, and
    no localhost hosts unless an explicit test/dev switch is set.
    """
    parsed = urlparse(url)
    scheme = parsed.scheme.lower()
    if scheme in FORBIDDEN_URL_SCHEMES:
        raise SystemExit(f"forbidden URL scheme in release index: {url!r}")
    if scheme != "https":
        raise SystemExit(
            f"release index URL must use https scheme, got {url!r} "
            f"(scheme={parsed.scheme!r})"
        )
    if not parsed.hostname:
        raise SystemExit(f"release index URL must have a non-empty host: {url!r}")
    if parsed.username or parsed.password:
        raise SystemExit(
            f"release index URL must not contain embedded credentials: {url!r}"
        )
    if parsed.fragment:
        # Fragments are not sent to the server and have no meaning for
        # release asset downloads; reject them to keep URLs deterministic.
        raise SystemExit(f"release index URL must not contain a fragment: {url!r}")
    hostname = parsed.hostname.lower().strip("[]")
    if hostname in LOCALHOST_HOSTS and not _localhost_allowed():
        raise SystemExit(
            f"release index URL must not point at localhost in production: {url!r} "
            f"(set RILL_ALLOW_LOCALHOST_URLS=1 for test/dev)"
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
    parser.add_argument("--handler-id", default="rillml.echo.handler")
    parser.add_argument("--handler-version")
    parser.add_argument("--handler-min-runtime", default=None)
    parser.add_argument("--output", type=Path, required=True)
    # The release channel distinguishes the stable pointer (``local-ai-stable``
    # with ``stable-index.json``) from the candidate pointer
    # (``local-ai-candidate`` with ``candidate-index.json``). A prerelease
    # version (e.g. ``1.0.0-rc.1``) must publish to the candidate channel
    # only; a stable version publishes to the stable channel. The channel
    # value is recorded in the signed payload so downstream clients can
    # reject a candidate index when they expect a stable one.
    parser.add_argument(
        "--channel",
        choices=("stable", "candidate"),
        default="stable",
    )
    args = parser.parse_args()

    base_url = f"https://github.com/{args.repository}/releases/download/{args.tag}"
    artifacts: list[dict[str, object]] = []
    for target_os, target_arch, pattern in RUNTIMES:
        name = pattern.format(version=args.version)
        asset_path = args.release_dir / name
        if not asset_path.is_file():
            # A platform asset may be intentionally skipped by the release
            # workflow (e.g. macOS builds are skipped when Apple Developer ID
            # secrets are not configured). The release index must not claim
            # support for a platform whose asset was not produced.
            continue
        url = f"{base_url}/{name}"
        validate_release_url(url)
        artifacts.append(
            artifact(
                asset_path,
                kind="runtime",
                id="rill-runtime",
                version=args.version,
                runtimeApiVersion=RUNTIME_API_VERSION,
                targetOs=target_os,
                targetArch=target_arch,
                url=url,
            )
        )

    existing_model = None
    existing_handler = None
    if args.existing_index and args.existing_index.is_file():
        envelope = json.loads(args.existing_index.read_text(encoding="utf-8"))
        for item in envelope["payload"]["artifacts"]:
            if item["kind"] == "model" and item["id"] == "rillml.example.default":
                existing_model = item
            elif item["kind"] == "handler" and item["id"] == args.handler_id:
                existing_handler = item
    if existing_model and semver_key(existing_model["version"]) > semver_key(args.version):
        # Re-validate URLs from the prior index rather than trusting them
        # blindly — a tampered existing index could otherwise smuggle a
        # non-HTTPS URL through the merge.
        validate_release_url(existing_model["url"])
        artifacts.append(existing_model)
    else:
        model_name = f"example-default-{args.version}.rillpack"
        url = f"{base_url}/{model_name}"
        validate_release_url(url)
        artifacts.append(
            artifact(
                args.release_dir / model_name,
                kind="model",
                id="rillml.example.default",
                version=args.version,
                runtimeApiVersion=RUNTIME_API_VERSION,
                url=url,
            )
        )

    handler_version = args.handler_version or args.version
    handler_min_runtime = args.handler_min_runtime or args.version
    handler_name = f"echo-handler-{handler_version}.rillhandler"
    handler_path = args.release_dir / handler_name
    if existing_handler and semver_key(existing_handler["version"]) > semver_key(handler_version):
        validate_release_url(existing_handler["url"])
        artifacts.append(existing_handler)
    elif handler_path.is_file():
        url = f"{base_url}/{handler_name}"
        validate_release_url(url)
        artifacts.append(
            artifact(
                handler_path,
                kind="handler",
                id=args.handler_id,
                version=handler_version,
                runtimeApiVersion=RUNTIME_API_VERSION,
                handlerApiVersion=HANDLER_API_VERSION,
                minRuntimeVersion=handler_min_runtime,
                url=url,
            )
        )

    payload = {
        "schemaVersion": RELEASE_INDEX_SCHEMA_VERSION,
        "channel": args.channel,
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
