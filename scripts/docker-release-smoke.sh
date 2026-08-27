#!/usr/bin/env bash
# RillML Docker-first release/smoke verification.
#
# Builds release artifacts for a first-class Linux target in Docker and runs a
# real smoke test against the freshly built rill-runtime / rill-pack binaries:
#   --help, create+verify a signed .rillpack, inspect-pack, and
#   handshake+invoke over IPC.
#
# The target runs inside a container built for the TARGET platform
# (--platform on docker build), so the binaries are built natively for that
# architecture and execute without relying on binfmt for the app binary
# (which cannot satisfy the target's dynamic loader). This mirrors
# scripts/cross-exec-test.sh.
#
# The smoke generates a fresh Ed25519 keypair, signs a real model package from
# models/example-default, verifies it back through `rill-pack verify` (which
# also proves the standalone pubkey derivation is consistent with
# ed25519-dalek), then serves an IPC handshake. This exercises the exact
# signed-package path the release pipeline produces.
#
# Usage:
#   ./scripts/docker-release-smoke.sh                 # x86_64 GNU
#   TARGET=aarch64-unknown-linux-gnu ./scripts/docker-release-smoke.sh
#
# docker-first principle: Linux release verification MUST run in Docker.

set -euo pipefail

cd "$(dirname "$0")/.."

RUST_PIN="${RUST_PIN:-1.97.0}"
TARGET="${TARGET:-x86_64-unknown-linux-gnu}"
IMAGE="rust:${RUST_PIN}-bookworm"

_platform() {
  case "$1" in
    x86_64-unknown-linux-gnu) echo "linux/amd64" ;;
    aarch64-unknown-linux-gnu) echo "linux/arm64" ;;
    *) echo "Unsupported target '$1' — add it to scripts/docker-release-smoke.sh" >&2; return 1 ;;
  esac
}
PLAT="$(_platform "$TARGET")" || exit 2

echo "==> Building smoke image for ${TARGET} (Docker platform ${PLAT})"
docker build --platform "${PLAT}" -t "rillml-smoke-${TARGET}:${RUST_PIN}" -f- . <<EOF
FROM ${IMAGE}
# python3 is required to derive the Ed25519 pubkey and read the pack manifest.
RUN apt-get update && apt-get install -y --no-install-recommends python3 && rm -rf /var/lib/apt/lists/*
EOF

echo "==> Building release binaries natively for ${TARGET} and running smoke"
docker run --rm -v "$PWD":/src -w /src "rillml-smoke-${TARGET}:${RUST_PIN}" bash -c "
  set -euo pipefail
  export PATH=\"/usr/local/cargo/bin:\$PATH\"
  cargo build --locked --release -p rill-runtime --bin rill-runtime --bin rill-pack --features wasm

  bin=target/release

  echo '-- rill-runtime --help'
  \$bin/rill-runtime --help >/dev/null

  echo '-- generate fresh Ed25519 keypair'
  seed=\$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')
  pubkey=\$(python3 scripts/derive_ed25519_pubkey.py \"\$seed\")
  echo \"  pubkey=\$pubkey\"

  # The trust store keys every signed pack by the key-id declared in the
  # manifest (publisherKeyId). Derive it so verify/serve match the manifest.
  key_id=\$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))[\"publisherKeyId\"])' models/example-default/manifest.json)
  echo \"  key_id=\$key_id\"

  echo '-- create a signed model pack'
  RILL_SIGNING_KEY_HEX=\"\$seed\" \$bin/rill-pack create \\
    --manifest models/example-default/manifest.json \\
    --model models/example-default/model.json \\
    --output /tmp/example.rillpack

  echo '-- verify the signed model pack (round-trips pubkey derivation)'
  \$bin/rill-pack verify \\
    --pack /tmp/example.rillpack \\
    --key-id \"\$key_id\" \\
    --public-key-hex \"\$pubkey\"

  echo '-- inspect-pack'
  \$bin/rill-runtime inspect-pack \\
    --pack /tmp/example.rillpack \\
    --model-trust-key \"\$key_id=\$pubkey\" >/dev/null

  echo '-- handshake over IPC (v2)'
  REQUEST='{\"method\":\"handshake\",\"requestId\":\"docker-smoke\",\"apiVersion\":2,\"clientName\":\"docker-smoke\",\"clientVersion\":\"0.0.0\"}'
  RESPONSE=\$(printf '%s\n' \"\$REQUEST\" | timeout 20 \$bin/rill-runtime serve \\
    --pack /tmp/example.rillpack \\
    --model-trust-key \"\$key_id=\$pubkey\" \\
    --builtin-handler linear-regression)
  echo \"\$RESPONSE\" | grep -q '\"kind\":\"handshake\"'
  echo \"\$RESPONSE\" | grep -q '\"apiVersion\":2'
  echo '-- release smoke PASSED'
"

echo "==> Release smoke PASSED for ${TARGET}"
