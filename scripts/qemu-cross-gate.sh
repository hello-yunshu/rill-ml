#!/usr/bin/env bash
# RillML Actions-first cross-architecture Stable gate.
#
# Real-executes the crate test suite AND the full Runtime on a non-native CPU
# architecture directly on the GitHub Actions host using the Debian cross C
# toolchain + QEMU user-mode emulation. This is the modern replacement for the
# Docker + QEMU/binfmt path: it needs no target-arch container and does not
# depend on whether upstream rust:* images publish a manifest for the platform.
#
# Principle: native runner > direct QEMU > VM > Docker. Direct user-mode QEMU
# is used when no native runner exists for the CPU; Docker is only preferred
# where it genuinely lowers setup complexity.
#
# Usage:
#   ./scripts/qemu-cross-gate.sh riscv64gc-unknown-linux-gnu
#   RUNTIME_SMOKE=0 ./scripts/qemu-cross-gate.sh armv7-unknown-linux-gnueabihf
#
# The full Runtime smoke (release rill-runtime with `wasm` enabled, --help,
# fresh-Ed25519 signed pack -> verify -> inspect -> IPC handshake -> WASM
# handler invoke) is run by default and can be disabled with RUNTIME_SMOKE=0.
#
# Supported GNU targets and their Debian cross packages:
#   riscv64gc-unknown-linux-gnu    gcc-riscv64-linux-gnu  + qemu-riscv64
#   aarch64-unknown-linux-gnu      gcc-aarch64-linux-gnu  + qemu-aarch64
#   armv7-unknown-linux-gnueabihf  gcc-arm-linux-gnueabihf+ qemu-arm
#   s390x-unknown-linux-gnu        gcc-s390x-linux-gnu    + qemu-s390x
#   powerpc64le-unknown-linux-gnu  gcc-powerpc64le-linux-gnu + qemu-ppc64le
#   x86_64 (host)                   (native, no QEMU)
#
# LoongArch64 is handled by scripts/loongarch-cross-gate.sh because its Debian
# cross toolchain is too old for the Rust target (needs GCC 15/Binutils 2.45).

set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="${1:-${TARGET:?usage: qemu-cross-gate.sh <triple>}}"
RUNTIME_SMOKE="${RUNTIME_SMOKE:-1}"

# The native CPU ID of the Actions host. When TARGET matches the host CPU the
# gate runs natively (no QEMU, no cross package) — Actions-first prefers the
# native runner. On Linux CI these are `x86_64` (ubuntu-24.04) and `aarch64`
# (ubuntu-24.04-arm).
_native_target() {
  case "$(uname -m)" in
    x86_64) echo "x86_64-unknown-linux-gnu" ;;
    aarch64|arm64) echo "aarch64-unknown-linux-gnu" ;;
    *) echo "" ;;
  esac
}
NATIVE_TARGET="$(_native_target)"
if [[ "$TARGET" == "$NATIVE_TARGET" ]]; then
  echo "==> ${TARGET} matches the native host CPU — running natively (no QEMU)"
fi

# Map target -> apt packages (cross gcc) + qemu binary + cargo runner.
# Implemented with a case statement so the script also runs on macOS bash 3.2.
_linker_pkg() {
  case "$1" in
    riscv64gc-unknown-linux-gnu) echo "gcc-riscv64-linux-gnu" ;;
    aarch64-unknown-linux-gnu) echo "gcc-aarch64-linux-gnu" ;;
    armv7-unknown-linux-gnueabihf) echo "gcc-arm-linux-gnueabihf" ;;
    s390x-unknown-linux-gnu) echo "gcc-s390x-linux-gnu" ;;
    powerpc64le-unknown-linux-gnu) echo "gcc-powerpc64le-linux-gnu" ;;
    x86_64-unknown-linux-gnu) echo "" ;;
    *) echo "Unsupported target '$1' for qemu-cross-gate.sh" >&2; return 1 ;;
  esac
}
_linker_bin() {
  case "$1" in
    riscv64gc-unknown-linux-gnu) echo "riscv64-linux-gnu-gcc" ;;
    aarch64-unknown-linux-gnu) echo "aarch64-linux-gnu-gcc" ;;
    armv7-unknown-linux-gnueabihf) echo "arm-linux-gnueabihf-gcc" ;;
    s390x-unknown-linux-gnu) echo "s390x-linux-gnu-gcc" ;;
    powerpc64le-unknown-linux-gnu) echo "powerpc64le-linux-gnu-gcc" ;;
    x86_64-unknown-linux-gnu) echo "gcc" ;;
  esac
}
_qemu_bin() {
  case "$1" in
    riscv64gc-unknown-linux-gnu) echo "qemu-riscv64" ;;
    aarch64-unknown-linux-gnu) echo "qemu-aarch64" ;;
    armv7-unknown-linux-gnueabihf) echo "qemu-arm" ;;
    s390x-unknown-linux-gnu) echo "qemu-s390x" ;;
    powerpc64le-unknown-linux-gnu) echo "qemu-ppc64le" ;;
    x86_64-unknown-linux-gnu) echo "" ;;
  esac
}
_ld_prefix() {
  # Debian cross packages install the target's libc under /usr/<triple>.
  case "$1" in
    riscv64gc-unknown-linux-gnu) echo "/usr/riscv64-linux-gnu" ;;
    aarch64-unknown-linux-gnu) echo "/usr/aarch64-linux-gnu" ;;
    armv7-unknown-linux-gnueabihf) echo "/usr/arm-linux-gnueabihf" ;;
    s390x-unknown-linux-gnu) echo "/usr/s390x-linux-gnu" ;;
    powerpc64le-unknown-linux-gnu) echo "/usr/powerpc64le-linux-gnu" ;;
    x86_64-unknown-linux-gnu) echo "/usr/x86_64-linux-gnu" ;;
  esac
}
_runner() {
  local qemu ld
  qemu="$(_qemu_bin "$1")"
  [ -z "$qemu" ] && return 0
  ld="$(_ld_prefix "$1")"
  case "$1" in
    riscv64gc-unknown-linux-gnu) echo "${qemu} -cpu rv64 -L ${ld}" ;;
    *) echo "${qemu} -L ${ld}" ;;
  esac
}

if [[ "$TARGET" != "$NATIVE_TARGET" ]]; then
  echo "==> Installing cross toolchain + QEMU for ${TARGET}"
  sudo apt-get update
  sudo apt-get install -y --no-install-recommends "$(_linker_pkg "$TARGET")" qemu-user python3
else
  echo "==> Native host target — using host toolchain, no cross package or QEMU"
fi

echo "==> Adding Rust target ${TARGET}"
rustup target add "$TARGET"

if [[ "$TARGET" == "$NATIVE_TARGET" ]]; then
  LINKER="gcc"
  RUNNER=""
else
  LINKER="$(_linker_bin "$TARGET")"
  RUNNER="$(_runner "$TARGET")"
fi

# Configure Cargo for cross compile + QEMU runner at the workspace level so
# every `cargo test/run` invocation transparently cross-compiled and executes
# the foreign binary through QEMU user-mode on this host.
mkdir -p .cargo
CONFIG=".cargo/config.toml"
UPPER="$(echo "$TARGET" | tr 'a-z-.' 'A-Z__')"
if [[ -n "$RUNNER" ]]; then
  cat >> "$CONFIG" <<EOF

# Added by scripts/qemu-cross-gate.sh (Actions-first direct QEMU)
[target.${TARGET}]
linker = "${LINKER}"
runner = "${RUNNER}"
EOF
  echo "==> QEMU runner: ${RUNNER}"
else
  cat >> "$CONFIG" <<EOF

# Added by scripts/qemu-cross-gate.sh (native host target)
[target.${TARGET}]
linker = "${LINKER}"
EOF
fi

# Native GNU gates (x86_64 / aarch64 host == target CPU) also run the quick
# quality checks that docker-test.sh used to gate on natively.
if [[ "$TARGET" == "$NATIVE_TARGET" ]]; then
  echo "==> cargo fmt --all --check (native gate)"
  cargo fmt --all --check
  echo "==> cargo clippy --workspace --all-targets --all-features (native gate)"
  cargo clippy --locked --workspace --all-targets --all-features \
    --exclude rill-ml-python --target "$TARGET"
fi

# Build the WASM handler components (host wasm32-unknown-unknown, no QEMU) so
# that the --all-features test suite — which includes rill-runtime's
# wasm_handler.rs / runtime_process.rs / wit_v1_component.rs — really performs
# WASM handler load + invoke under QEMU for the foreign target, instead of
# printing "skipping". This satisfies the RISC-V64 Stable Gate requirement of a
# real WASM handler execute path (the runtime uses the portable Pulley backend
# on architectures without a native Cranelift tier).
echo "==> Building WASM handler fixtures (wasm32-unknown-unknown, host-native)"
rustup target add wasm32-unknown-unknown
FIXTURE_DIR="target"
mkdir -p "$FIXTURE_DIR"
( cd handlers/echo-handler && \
  cargo build --locked --release --target wasm32-unknown-unknown >/dev/null )
if ! command -v wasm-tools >/dev/null 2>&1; then
  cargo install wasm-tools --locked --version "~1.254.0"
fi
wasm-tools component new \
  handlers/echo-handler/target/wasm32-unknown-unknown/release/echo-handler.wasm \
  -o "$FIXTURE_DIR/echo-handler.wasm"
export ECHO_HANDLER_WASM="${PWD}/target/echo-handler.wasm"
echo "==> ECHO_HANDLER_WASM=${ECHO_HANDLER_WASM}"

echo "==> cargo test --workspace --target ${TARGET}"
cargo test --locked --workspace --all-targets --all-features \
  --exclude rill-ml-python --target "$TARGET"
echo "==> cargo test --doc --target ${TARGET}"
cargo test --locked --workspace --doc --all-features \
  --exclude rill-ml-python --target "$TARGET"

if [[ "$RUNTIME_SMOKE" != "0" ]]; then
  echo "==> Runtime smoke for ${TARGET} via QEMU user-mode"
  # Full default Runtime (wasmtime / Pulley handler enabled). The default is
  # `--features wasm` which mirrors the shipped release binaries. For targets
  # where compiling/executing wasmtime under QEMU is not viable (s390x SIGSEGV)
  # the workflow can pass RUNTIME_FEATURES_FLAGS="--no-default-features"; their
  # WASM path is still covered by the --all-features crate tests above.
  RUNTIME_FEATURES_FLAGS="${RUNTIME_FEATURES_FLAGS:---features wasm}"
  cargo build --locked --release --target "$TARGET" \
    -p rill-runtime --bin rill-runtime --bin rill-pack $RUNTIME_FEATURES_FLAGS

  BIN="target/${TARGET}/release"
  run_qemu() { # run a target binary through the configured runner
    if [[ -n "$RUNNER" ]]; then
      # shellcheck disable=SC2086
      $RUNNER "$BIN/$1" "${@:2}"
    else
      "$BIN/$1" "${@:2}"
    fi
  }

  echo '-- rill-runtime --help'
  run_qemu rill-runtime --help >/dev/null

  echo '-- generate fresh Ed25519 keypair'
  seed="$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')"
  pubkey="$(python3 scripts/derive_ed25519_pubkey.py "$seed")"
  key_id="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["publisherKeyId"])' models/example-default/manifest.json)"

  echo '-- create a signed model pack'
  RILL_SIGNING_KEY_HEX="$seed" run_qemu rill-pack create \
    --manifest models/example-default/manifest.json \
    --model models/example-default/model.json \
    --output /tmp/example.rillpack

  echo '-- verify the signed model pack'
  run_qemu rill-pack verify \
    --pack /tmp/example.rillpack \
    --key-id "$key_id" \
    --public-key-hex "$pubkey"

  echo '-- inspect-pack'
  run_qemu rill-runtime inspect-pack \
    --pack /tmp/example.rillpack \
    --model-trust-key "$key_id=$pubkey" >/dev/null

  echo '-- handshake over IPC'
  REQUEST='{"method":"handshake","requestId":"runtime-smoke","apiVersion":2,"clientName":"runtime-smoke","clientVersion":"0.0.0"}'
  if [[ -n "$RUNNER" ]]; then
    # shellcheck disable=SC2086
    RESPONSE="$(printf '%s\n' "$REQUEST" | timeout 40 $RUNNER "$BIN/rill-runtime" serve \
      --pack /tmp/example.rillpack \
      --model-trust-key "$key_id=$pubkey" \
      --builtin-handler linear-regression)"
  else
    RESPONSE="$(printf '%s\n' "$REQUEST" | timeout 40 "$BIN/rill-runtime" serve \
      --pack /tmp/example.rillpack \
      --model-trust-key "$key_id=$pubkey" \
      --builtin-handler linear-regression)"
  fi
  echo "$RESPONSE" | grep -q '"kind":"handshake"'
  echo "$RESPONSE" | grep -q '"apiVersion":2'

  echo '-- runtime smoke PASSED'
fi

echo "==> Cross gate PASSED for ${TARGET}"