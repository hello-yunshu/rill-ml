#!/usr/bin/env bash
# Linux musl Stable gate for targets without a native GitHub runner.
#
# This gate proves the supported Core and default Full Runtime surfaces with
# direct target-architecture execution. MIPS/MIPSel are intentionally absent:
# their Rust standard-library components are not distributable by rustup on the
# pinned release toolchains available to this project.
#
# Usage:
#   ./scripts/musl-cross-gate.sh riscv64gc-unknown-linux-musl
#   ./scripts/musl-cross-gate.sh armv7-unknown-linux-musleabihf

set -Eeuo pipefail

stage="bootstrap"
trap 'rc=$?; echo "::error::musl gate failed target=${TARGET:-unknown} stage=${stage} rc=${rc}" >&2; exit "$rc"' ERR

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

serve_capture() {
  local label="$1"
  local input="$2"
  local output
  local rc
  if output="$(printf '%s\n' "$input" | timeout 120 "${QEMU}" "${QEMU_ARGS[@]}" "$BIN/rill-runtime" serve \
    --pack /tmp/example.rillpack --model-trust-key "$key_id=$pubkey" \
    --handler /tmp/echo.rillhandler --handler-trust-key "$key_id=$pubkey" 2>&1)"; then
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

stage="install-tools"
echo "==> Install cross execution tools for ${TARGET}"
sudo apt-get update
sudo apt-get install -y --no-install-recommends qemu-user python3 file
command -v zig >/dev/null 2>&1 || {
  echo "musl-cross-gate: Zig is required; install it in the Actions job first" >&2
  exit 1
}

stage="rust-target"
echo "==> Add Rust target ${TARGET}"
rustup target add "$TARGET"

export RILL_ZIG_TARGET="$ZIG_TARGET"
target_key="$(printf '%s' "$TARGET" | tr '[:lower:]-' '[:upper:]_')"
export "CARGO_TARGET_${target_key}_LINKER=${PWD}/scripts/zig-target-linker.sh"
cc_target_key="$(printf '%s' "$TARGET" | tr '-' '_')"
export "CC_${cc_target_key}=${PWD}/scripts/zig-target-cc.sh"
runner="${QEMU} ${QEMU_ARGS[*]}"
export "CARGO_TARGET_${target_key}_RUNNER=${runner}"

stage="build-handler-fixture"
echo "==> Build host-native WASM handler fixture"
rustup target add wasm32-unknown-unknown
cargo build --locked --release --manifest-path handlers/echo-handler/Cargo.toml \
  --target wasm32-unknown-unknown
command -v wasm-tools >/dev/null 2>&1 || cargo install wasm-tools --locked --version "1.254.0"
mkdir -p target
stage="componentize-handler"
wasm-tools component new \
  handlers/echo-handler/target/wasm32-unknown-unknown/release/echo-handler.wasm \
  -o target/echo-handler.wasm
export ECHO_HANDLER_WASM="${PWD}/target/echo-handler.wasm"

export RILL_RUNTIME_EXEC_PREFIX="${runner}"
echo "==> Target: ${TARGET}; Zig target: ${ZIG_TARGET}; runner: ${runner}"

stage="core-tests"
echo "==> Core compile/test gate under QEMU"
/usr/bin/time -f 'core_max_rss_kib=%M' \
  cargo test --locked \
  -p rill-ml -p rill-handler-api -p rill-runtime-protocol \
  --all-targets --all-features --target "$TARGET"
stage="core-docs"
/usr/bin/time -f 'doc_max_rss_kib=%M' \
  cargo test --locked --doc \
  -p rill-ml -p rill-handler-api -p rill-runtime-protocol \
  --all-features --target "$TARGET"

if [[ "$RUNTIME_SMOKE" == "1" ]]; then
  stage="runtime-build"
  echo "==> Full Runtime Stable gate under QEMU"
  /usr/bin/time -f 'runtime_max_rss_kib=%M' \
    cargo build --locked --release --target "$TARGET" \
    -p rill-runtime --bin rill-runtime --bin rill-pack --features wasm
  BIN="target/${TARGET}/release"
  stage="runtime-help"
  "${QEMU}" "${QEMU_ARGS[@]}" "$BIN/rill-runtime" --help >/dev/null
  seed="$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')"
  pubkey="$(python3 scripts/derive_ed25519_pubkey.py "$seed")"
  key_id="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["publisherKeyId"])' models/example-default/manifest.json)"
  stage="model-pack-create"
  RILL_SIGNING_KEY_HEX="$seed" "${QEMU}" "${QEMU_ARGS[@]}" "$BIN/rill-pack" create \
    --manifest models/example-default/manifest.json \
    --model models/example-default/model.json --output /tmp/example.rillpack
  stage="model-pack-verify"
  "${QEMU}" "${QEMU_ARGS[@]}" "$BIN/rill-pack" verify \
    --pack /tmp/example.rillpack --key-id "$key_id" --public-key-hex "$pubkey"
  stage="model-pack-inspect"
  "${QEMU}" "${QEMU_ARGS[@]}" "$BIN/rill-runtime" inspect-pack \
    --pack /tmp/example.rillpack --model-trust-key "$key_id=$pubkey"
  stage="handler-pack-create"
  RILL_SIGNING_KEY_HEX="$seed" "${QEMU}" "${QEMU_ARGS[@]}" "$BIN/rill-pack" create-handler \
    --manifest handlers/echo-handler/manifest.json \
    --module "$ECHO_HANDLER_WASM" \
    --output /tmp/echo.rillhandler
  stage="handler-pack-inspect"
  "${QEMU}" "${QEMU_ARGS[@]}" "$BIN/rill-pack" inspect-handler \
    --handler /tmp/echo.rillhandler --key-id "$key_id" --public-key-hex "$pubkey"
  request='{"method":"handshake","requestId":"musl-cross","apiVersion":2,"clientName":"musl-cross","clientVersion":"0.0.0"}'
  health='{"method":"health","requestId":"musl-health","apiVersion":2}'
  invoke='{"method":"invoke","requestId":"musl-invoke","apiVersion":2,"capability":"rillml.linearRegression.predict","input":{"features":[1.0]}}'
  malformed='not-json'
  stage="runtime-handshake"
  serve_capture handshake "$request"
  response="$RUNTIME_RESPONSE"
  assert_contains handshake-kind "$response" '"kind":"handshake"'
  assert_contains handshake-api-version "$response" '"apiVersion":2'
  stage="runtime-health"
  serve_capture health "$health"
  response="$RUNTIME_RESPONSE"
  assert_contains health-kind "$response" '"kind":"health"'
  stage="runtime-invoke"
  serve_capture invoke "$invoke"
  response="$RUNTIME_RESPONSE"
  assert_contains invoke-kind "$response" '"kind":"invoke"'
  assert_contains invoke-features "$response" '"features"'
  stage="runtime-malformed-json"
  serve_capture malformed-json "$malformed"
  response="$RUNTIME_RESPONSE"
  assert_contains malformed-json "$response" '"invalidJson"'
fi

stage="final"
echo "==> Linux musl Stable gate PASSED for ${TARGET}"
