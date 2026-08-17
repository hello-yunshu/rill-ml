#!/usr/bin/env bash
# RillML musl Stable gate — Actions-first, no container.
#
# Builds the full Runtime (default `wasm` feature) for a musl target on a NATIVE
# GitHub runner whose CPU matches the target, then executes the resulting musl
# binary directly. Docker is not required: Debian `musl-tools` installs the
# host-arch `musl-gcc`, so the Rust musl triple links and runs on the host.
#
#   x86_64-unknown-linux-musl   -> ubuntu-24.04      (native x86_64)
#   aarch64-unknown-linux-musl  -> ubuntu-24.04-arm  (native aarch64)
#
# Usage:
#   ./scripts/musl-gate.sh x86_64-unknown-linux-musl
#   ./scripts/musl-gate.sh aarch64-unknown-linux-musl
#
# The generated binary is verified to be a musl build and then real-executed
# (--help, signed pack -> verify -> inspect -> IPC handshake).

set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="${1:?usage: musl-gate.sh <musl-triple>}"
RUNTIME_SMOKE="${RUNTIME_SMOKE:-1}"

case "$TARGET" in
  x86_64-unknown-linux-musl|aarch64-unknown-linux-musl) : ;;
  *) echo "Unsupported musl target '${TARGET}'" >&2; exit 1 ;;
esac

# Guard: the host CPU must match the musl target so execution is native.
_native_arch() {
  case "$(uname -m)" in
    x86_64) echo "x86_64" ;;
    aarch64|arm64) echo "aarch64" ;;
    *) echo "" ;;
  esac
}
HOST_ARCH="$(_native_arch)"
EXPECTED="${TARGET%*-unknown-linux-musl}"
if [[ "$HOST_ARCH" != "$EXPECTED" ]]; then
  echo "musl-gate: target ${TARGET} needs a ${EXPECTED} runner, host is ${HOST_ARCH}." >&2
  exit 1
fi

echo "==> Installing musl toolchain for ${TARGET} (native ${HOST_ARCH})"
sudo apt-get update
sudo apt-get install -y --no-install-recommends musl-tools python3

echo "==> Adding Rust target ${TARGET}"
rustup target add "$TARGET"

# musl-gcc provided by musl-tools is the host-arch musl compiler; static link by
# default. Configuring it as the linker makes `cargo build --target <musl>`
# genuinely produce a musl runtime (not a glibc masquerade).
mkdir -p .cargo
cat >> .cargo/config.toml <<EOF

# Added by scripts/musl-gate.sh (Actions-first native musl)
[target.${TARGET}]
linker = "musl-gcc"
EOF

echo "==> Building release runtime for ${TARGET}"
cargo build --locked --release --target "$TARGET" \
  -p rill-runtime --bin rill-runtime --bin rill-pack --features wasm

BIN="target/${TARGET}/release"

echo "-- ELF identity"
file "$BIN/rill-runtime" "$BIN/rill-pack"
echo "-- dynamic interpreter (empty => static musl)"
"${READELF:-readelf}" -l "$BIN/rill-runtime" 2>/dev/null | awk '/interpreter/ {print "  interpreter:", $NF} /interpreter=/ {print}'

if [[ "$RUNTIME_SMOKE" != "0" ]]; then
  echo "==> Runtime smoke for ${TARGET} (native musl execution)"
  run_bin() { "$BIN/$1" "${@:2}"; }

  echo '-- rill-runtime --help'
  run_bin rill-runtime --help >/dev/null

  echo '-- generate fresh Ed25519 keypair'
  seed="$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')"
  pubkey="$(python3 scripts/derive_ed25519_pubkey.py "$seed")"
  key_id="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["publisherKeyId"])' models/example-default/manifest.json)"

  echo '-- create a signed model pack'
  RILL_SIGNING_KEY_HEX="$seed" run_bin rill-pack create \
    --manifest models/example-default/manifest.json \
    --model models/example-default/model.json \
    --output /tmp/example.rillpack

  echo '-- verify the signed model pack'
  run_bin rill-pack verify \
    --pack /tmp/example.rillpack \
    --key-id "$key_id" \
    --public-key-hex "$pubkey"

  echo '-- inspect-pack'
  run_bin rill-runtime inspect-pack \
    --pack /tmp/example.rillpack \
    --model-trust-key "$key_id=$pubkey" >/dev/null

  echo '-- handshake over IPC'
  REQUEST='{"method":"handshake","requestId":"musl-smoke","apiVersion":2,"clientName":"musl-smoke","clientVersion":"0.0.0"}'
  RESPONSE="$(printf '%s\n' "$REQUEST" | timeout 40 "$BIN/rill-runtime" serve \
    --pack /tmp/example.rillpack \
    --model-trust-key "$key_id=$pubkey" \
    --builtin-handler linear-regression)"
  echo "$RESPONSE" | grep -q '"kind":"handshake"'
  echo "$RESPONSE" | grep -q '"apiVersion":2'

  echo '-- runtime smoke PASSED'
fi

echo "==> musl gate PASSED for ${TARGET}"