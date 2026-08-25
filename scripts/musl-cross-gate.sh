#!/usr/bin/env bash
# Linux musl Stable gate for targets without a native GitHub runner.
#
# This gate proves the supported Core and default Full Runtime surfaces with
# direct target-architecture execution. MIPS/MIPSel are intentionally absent:
# their Rust 1.94 standard-library components are not distributable by rustup.
#
# Usage:
#   ./scripts/musl-cross-gate.sh riscv64gc-unknown-linux-musl
#   ./scripts/musl-cross-gate.sh armv7-unknown-linux-musleabihf

set -euo pipefail

cd "$(dirname "$0")/.."
TARGET="${1:?usage: musl-cross-gate.sh <musl-triple>}"
RUNTIME_SMOKE="${RUNTIME_SMOKE:-1}"

case "$TARGET" in
  riscv64gc-unknown-linux-musl)
    ZIG_TARGET="riscv64-linux-musl"
    QEMU="qemu-riscv64"
    QEMU_ARGS=(-cpu rv64)
    ;;
  armv7-unknown-linux-musleabihf)
    ZIG_TARGET="arm-linux-musleabihf"
    QEMU="qemu-arm"
    QEMU_ARGS=()
    ;;
  i686-unknown-linux-musl)
    ZIG_TARGET="x86-linux-musl"
    QEMU="qemu-i386"
    QEMU_ARGS=()
    ;;
  *)
    echo "musl-cross-gate: unsupported target '${TARGET}'" >&2
    exit 1
    ;;
esac

echo "==> Install cross execution tools for ${TARGET}"
sudo apt-get update
sudo apt-get install -y --no-install-recommends qemu-user python3 file
command -v zig >/dev/null 2>&1 || {
  echo "musl-cross-gate: Zig is required; install it in the Actions job first" >&2
  exit 1
}

echo "==> Add Rust target ${TARGET}"
rustup target add "$TARGET"

export RILL_ZIG_TARGET="$ZIG_TARGET"
target_key="$(printf '%s' "$TARGET" | tr '[:lower:]-' '[:upper:]_')"
export "CARGO_TARGET_${target_key}_LINKER=${PWD}/scripts/zig-target-linker.sh"
export "CC_${target_key}=${PWD}/scripts/zig-target-cc.sh"
runner="${QEMU} ${QEMU_ARGS[*]}"
export "CARGO_TARGET_${target_key}_RUNNER=${runner}"

echo "==> Build host-native WASM handler fixture"
rustup target add wasm32-unknown-unknown
cargo build --locked --release --manifest-path handlers/echo-handler/Cargo.toml \
  --target wasm32-unknown-unknown
command -v wasm-tools >/dev/null 2>&1 || cargo install wasm-tools --locked --version "1.254.0"
mkdir -p target
wasm-tools component new \
  handlers/echo-handler/target/wasm32-unknown-unknown/release/echo-handler.wasm \
  -o target/echo-handler.wasm
export ECHO_HANDLER_WASM="${PWD}/target/echo-handler.wasm"

export RILL_RUNTIME_EXEC_PREFIX="${runner}"
echo "==> Target: ${TARGET}; Zig target: ${ZIG_TARGET}; runner: ${runner}"

echo "==> Core compile/test/doc gate under QEMU"
/usr/bin/time -f 'core_max_rss_kib=%M' \
  cargo test --locked \
  -p rill-ml -p rill-handler-api -p rill-runtime-protocol \
  --all-targets --all-features --target "$TARGET"
/usr/bin/time -f 'doc_max_rss_kib=%M' \
  cargo test --locked --workspace --doc --all-features \
  --exclude rill-ml-python --target "$TARGET"

if [[ "$RUNTIME_SMOKE" == "1" ]]; then
  echo "==> Full Runtime Stable gate under QEMU"
  /usr/bin/time -f 'runtime_max_rss_kib=%M' \
    cargo build --locked --release --target "$TARGET" \
    -p rill-runtime --bin rill-runtime --bin rill-pack --features wasm
  BIN="target/${TARGET}/release"
  "${QEMU}" "${QEMU_ARGS[@]}" "$BIN/rill-runtime" --help >/dev/null
  seed="$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')"
  pubkey="$(python3 scripts/derive_ed25519_pubkey.py "$seed")"
  key_id="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["publisherKeyId"])' models/example-default/manifest.json)"
  RILL_SIGNING_KEY_HEX="$seed" "${QEMU}" "${QEMU_ARGS[@]}" "$BIN/rill-pack" create \
    --manifest models/example-default/manifest.json \
    --model models/example-default/model.json --output /tmp/example.rillpack
  "${QEMU}" "${QEMU_ARGS[@]}" "$BIN/rill-pack" verify \
    --pack /tmp/example.rillpack --key-id "$key_id" --public-key-hex "$pubkey"
  "${QEMU}" "${QEMU_ARGS[@]}" "$BIN/rill-runtime" inspect-pack \
    --pack /tmp/example.rillpack --model-trust-key "$key_id=$pubkey" >/dev/null
  RILL_SIGNING_KEY_HEX="$seed" "${QEMU}" "${QEMU_ARGS[@]}" "$BIN/rill-pack" create-handler \
    --manifest handlers/echo-handler/manifest.json \
    --module "$ECHO_HANDLER_WASM" \
    --output /tmp/echo.rillhandler
  "${QEMU}" "${QEMU_ARGS[@]}" "$BIN/rill-pack" inspect-handler \
    --handler /tmp/echo.rillhandler --key-id "$key_id" --public-key-hex "$pubkey" >/dev/null
  request='{"method":"handshake","requestId":"musl-cross","apiVersion":2,"clientName":"musl-cross","clientVersion":"0.0.0"}'
  health='{"method":"health","requestId":"musl-health","apiVersion":2}'
  invoke='{"method":"invoke","requestId":"musl-invoke","apiVersion":2,"capability":"rillml.linearRegression.predict","input":{"features":[1.0]}}'
  malformed='not-json'
  response="$(printf '%s\n' "$request" | timeout 120 "${QEMU}" "${QEMU_ARGS[@]}" "$BIN/rill-runtime" serve \
    --pack /tmp/example.rillpack --model-trust-key "$key_id=$pubkey" \
    --handler /tmp/echo.rillhandler --handler-trust-key "$key_id=$pubkey")"
  echo "$response" | grep -q '"kind":"handshake"'
  echo "$response" | grep -q '"apiVersion":2'
  responses="$(printf '%s\n%s\n%s\n%s\n' "$request" "$health" "$invoke" "$malformed" | timeout 120 "${QEMU}" "${QEMU_ARGS[@]}" "$BIN/rill-runtime" serve \
    --pack /tmp/example.rillpack --model-trust-key "$key_id=$pubkey" \
    --handler /tmp/echo.rillhandler --handler-trust-key "$key_id=$pubkey")"
  echo "$responses" | grep -q '"kind":"health"'
  echo "$responses" | grep -q '"kind":"invoke"'
  echo "$responses" | grep -q '"features"'
  echo "$responses" | grep -q '"invalidJson"'
fi

echo "==> Linux musl Stable gate PASSED for ${TARGET}"
