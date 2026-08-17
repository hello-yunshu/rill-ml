#!/usr/bin/env python3
"""Replace only one model artifact in an already verified stable index."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
from urllib.parse import urlparse


FORBIDDEN_URL_SCHEMES = frozenset({"file", "data", "javascript", "ftp", "blob"})

# Hostnames that resolve to the local machine. Production release indexes
# must never point at localhost. See ``build-release-index.py`` for the
# full rationale and the ``RILL_ALLOW_LOCALHOST_URLS`` opt-in.
LOCALHOST_HOSTS = frozenset({"localhost", "127.0.0.1", "::1", "0.0.0.0"})


def _localhost_allowed() -> bool:
    """Return True if the environment explicitly allows localhost URLs.

    Mirrors the gate in ``build-release-index.py`` so the two scripts
    cannot drift on localhost policy.
    """
    return os.environ.get("RILL_ALLOW_LOCALHOST_URLS", "") == "1"


def validate_release_url(url: str) -> None:
    """Reject URLs that are not HTTPS with a non-empty host and no userinfo.

    Mirrors the policy in ``build-release-index.py`` so a model-only update
    cannot smuggle in a URL scheme the bootstrap release would have refused.
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
        raise SystemExit(f"release index URL must not contain a fragment: {url!r}")
    hostname = parsed.hostname.lower().strip("[]")
    if hostname in LOCALHOST_HOSTS and not _localhost_allowed():
        raise SystemExit(
            f"release index URL must not point at localhost in production: {url!r} "
            f"(set RILL_ALLOW_LOCALHOST_URLS=1 for test/dev)"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--signed-index", type=pathlib.Path, required=True)
    parser.add_argument("--model", type=pathlib.Path, required=True)
    parser.add_argument("--model-id", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--url", required=True)
    parser.add_argument("--publisher-key-id", required=True)
    parser.add_argument("--generated-at", required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()

    validate_release_url(args.url)

    envelope = json.loads(args.signed_index.read_text(encoding="utf-8"))
    payload = envelope["payload"]
    if payload["publisherKeyId"] != args.publisher_key_id:
        raise SystemExit("existing index publisher does not match")
    # Preserve the channel of the existing index so a model-only update
    # cannot accidentally downgrade a candidate index to stable or vice
    # versa. The bootstrap release sets the channel; subsequent model-only
    # updates inherit it.
    existing_channel = payload.get("channel", "stable")
    runtimes = [item for item in payload["artifacts"] if item["kind"] == "runtime"]
    if not runtimes:
        raise SystemExit("stable index has no runtime; publish the bootstrap runtime release first")
    retained = [
        item
        for item in payload["artifacts"]
        if not (item["kind"] == "model" and item["id"] == args.model_id)
    ]
    # Re-validate every retained URL so a previously-signed index cannot
    # smuggle a forbidden scheme into the new model-only payload.
    for item in retained:
        if "url" in item:
            validate_release_url(item["url"])
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
        "schemaVersion": 3,
        "channel": existing_channel,
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
