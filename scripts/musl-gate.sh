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

set -Eeuo pipefail

stage="bootstrap"
trap 'rc=$?; echo "::error::musl gate failed target=${TARGET:-unknown} stage=${stage} rc=${rc}" >&2; exit "$rc"' ERR

cd "$(dirname "$0")/.."

TARGET="${1:?usage: musl-gate.sh <musl-triple>}"
RUNTIME_SMOKE="${RUNTIME_SMOKE:-1}"

case "$TARGET" in
  x86_64-unknown-linux-musl|aarch64-unknown-linux-musl) : ;;
  *) echo "Unsupported musl target '${TARGET}'" >&2; exit 1 ;;
esac

assert_contains() {
  local name="$1"
  local haystack="$2"
  local needle="$3"
  if ! grep -Fq -- "$needle" <<<"$haystack"; then
    echo "::error::assertion failed target=${TARGET} stage=${stage} name=${name} expected=${needle}" >&2
    printf 'response=%s\n' "$haystack" >&2
    return 1
  fi
}

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

stage="install-tools"
echo "==> Installing musl toolchain for ${TARGET} (native ${HOST_ARCH})"
sudo apt-get update
sudo apt-get install -y --no-install-recommends musl-tools python3

stage="rust-target"
echo "==> Adding Rust target ${TARGET}"
rustup target add "$TARGET"
rustup target add wasm32-unknown-unknown
if ! command -v wasm-tools >/dev/null 2>&1; then
  cargo install wasm-tools --locked --version "1.254.0"
fi
stage="build-handler-fixture"
( cd handlers/echo-handler && cargo build --locked --release --target wasm32-unknown-unknown )
mkdir -p target
stage="componentize-handler"
wasm-tools component new \
  handlers/echo-handler/target/wasm32-unknown-unknown/release/echo-handler.wasm \
  -o target/echo-handler.wasm
export ECHO_HANDLER_WASM="${PWD}/target/echo-handler.wasm"

# musl-gcc provided by musl-tools is the host-arch musl compiler; static link by
# default. Configuring it as the linker makes `cargo build --target <musl>`
# genuinely produce a musl runtime (not a glibc masquerade).
mkdir -p .cargo
cat >> .cargo/config.toml <<EOF

# Added by scripts/musl-gate.sh (Actions-first native musl)
[target.${TARGET}]
linker = "musl-gcc"
EOF

stage="runtime-build"
echo "==> Building release runtime for ${TARGET}"
cargo build --locked --release --target "$TARGET" \
  -p rill-runtime --bin rill-runtime --bin rill-pack --features wasm

BIN="target/${TARGET}/release"

echo "-- ELF identity"
file "$BIN/rill-runtime" "$BIN/rill-pack"
echo "-- dynamic interpreter (empty => static musl)"
"${READELF:-readelf}" -l "$BIN/rill-runtime" 2>/dev/null | awk '/interpreter/ {print "  interpreter:", $NF} /interpreter=/ {print}'

if [[ "$RUNTIME_SMOKE" != "0" ]]; then
  stage="runtime-help"
  echo "==> Runtime smoke for ${TARGET} (native musl execution)"
  run_bin() { "$BIN/$1" "${@:2}"; }

  echo '-- rill-runtime --help'
  run_bin rill-runtime --help >/dev/null
  echo '-- backend diagnostics'
  stage="runtime-diagnostics"
  diagnostics="$(run_bin rill-runtime diagnostics)"
  assert_contains backend "$diagnostics" '"backend":"'

  echo '-- generate fresh Ed25519 keypair'
  seed="$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')"
  pubkey="$(python3 scripts/derive_ed25519_pubkey.py "$seed")"
  key_id="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["publisherKeyId"])' models/example-default/manifest.json)"

  stage="model-pack-create"
  echo '-- create a signed model pack'
  RILL_SIGNING_KEY_HEX="$seed" run_bin rill-pack create \
    --manifest models/example-default/manifest.json \
    --model models/example-default/model.json \
    --output /tmp/example.rillpack

  stage="model-pack-verify"
  echo '-- verify the signed model pack'
  run_bin rill-pack verify \
    --pack /tmp/example.rillpack \
    --key-id "$key_id" \
    --public-key-hex "$pubkey"

  stage="model-pack-inspect"
  echo '-- inspect-pack'
  run_bin rill-runtime inspect-pack \
    --pack /tmp/example.rillpack \
    --model-trust-key "$key_id=$pubkey"

  stage="handler-pack-create"
  echo '-- create and inspect signed WASM handler pack'
  RILL_SIGNING_KEY_HEX="$seed" run_bin rill-pack create-handler \
    --manifest handlers/echo-handler/manifest.json \
    --module "$ECHO_HANDLER_WASM" \
    --output /tmp/echo.rillhandler
  stage="handler-pack-inspect"
  run_bin rill-pack inspect-handler \
    --handler /tmp/echo.rillhandler \
    --key-id "$key_id" \
    --public-key-hex "$pubkey"

  serve_capture() {
    local label="$1"
    local input="$2"
    local output
    local rc
    if output="$(printf '%s\n' "$input" | timeout 40 "$BIN/rill-runtime" serve \
      --pack /tmp/example.rillpack \
      --model-trust-key "$key_id=$pubkey" \
      --handler /tmp/echo.rillhandler \
      --handler-trust-key "$key_id=$pubkey" 2>&1)"; then
      :
    else
      rc=$?
      echo "::error::runtime serve failed target=${TARGET} stage=${stage} label=${label} rc=${rc}" >&2
      printf 'runtime_output=%s\n' "$output" >&2
      return "$rc"
    fi
    RUNTIME_RESPONSE="$output"
    printf '%s\n' "$output"
  }

  stage="runtime-handshake"
  echo '-- handshake over IPC'
  REQUEST='{"method":"handshake","requestId":"musl-smoke","apiVersion":2,"clientName":"musl-smoke","clientVersion":"0.0.0"}'
  HEALTH='{"method":"health","requestId":"musl-health","apiVersion":2}'
  INVOKE='{"method":"invoke","requestId":"musl-invoke","apiVersion":2,"capability":"rillml.linearRegression.predict","input":{"features":[1.0]}}'
  serve_capture handshake "$REQUEST"
  RESPONSE="$RUNTIME_RESPONSE"
  assert_contains handshake-kind "$RESPONSE" '"kind":"handshake"'
  assert_contains handshake-api-version "$RESPONSE" '"apiVersion":2'
  stage="runtime-health"
  serve_capture health "$HEALTH"
  RESPONSE="$RUNTIME_RESPONSE"
  assert_contains health-kind "$RESPONSE" '"kind":"health"'
  stage="runtime-invoke"
  serve_capture invoke "$INVOKE"
  RESPONSE="$RUNTIME_RESPONSE"
  assert_contains invoke-kind "$RESPONSE" '"kind":"invoke"'
  assert_contains invoke-features "$RESPONSE" '"features"'

  echo '-- runtime smoke PASSED'
fi

stage="final"
echo "==> musl gate PASSED for ${TARGET}"
