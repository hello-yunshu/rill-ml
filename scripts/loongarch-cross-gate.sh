#!/usr/bin/env bash
# RillML LoongArch64 Stable gate — Actions-first, no Docker.
#
# Real-executes the LoongArch64 crate test suite AND the full Runtime (default
# `wasm` feature → Wasmtime portable / Pulley backend) directly on the GitHub
# Actions host using a pinned, checksum-verified LoongArch GNU cross toolchain
# + QEMU user-mode. This replaces the old Docker-manifest-dependent path, which
# could not run LoongArch because upstream rust:* publishes no
# linux/loongarch64 image manifest.
#
# Principle: native runner > direct QEMU > VM > Docker. LoongArch has no native
# GitHub runner, so we use direct user-mode QEMU with an official cross
# toolchain. If user-mode proves insufficient for Wasmtime/Pulley we would
# escalate to qemu-system-loongarch64 (see PLATFORM_SUPPORT.md).
#
# Toolchain (pinned — never a moving `latest`):
#   repo    : loong64/cross-tools, release 20260812
#   asset   : x86_64-cross-tools-loongarch64-unknown-linux-gnu-latest.tar.xz
#   sha256  : adf5a3235465f936eff045a4098c7acb445370b53413fcecb41938755b093f5c
#   gcc     : 16.2.0, binutils 2.47, glibc 2.44, kernel 6.16  (latest profile)
#   layout  : crosstool-ng style; we locate <triple>-gcc and use `-print-sysroot`
#
# We use the "latest" profile (binutils 2.47) rather than "stable" (2.41):
# LLVM-generated LoongArch objects (e.g. polars) use newer R_LARCH_* relaxation
# / TLS relocations (0x6e = 110) that GNU ld 2.41 cannot link
# (`unsupported relocation type 0x6e`). Binutils >= 2.43 understands them.
#
# Usage:
#   ./scripts/loongarch-cross-gate.sh        # full gate (test + runtime)
#
# The toolchain is cached under $HOME/.rillml-loongarch-toolchain so the
# workflow can persist it with actions/cache between runs.

set -euo pipefail

cd "$(dirname "$0")/.."

HOST_ARCH="$(uname -m)"
TARGET="loongarch64-unknown-linux-gnu"
RUNTIME_SMOKE="${RUNTIME_SMOKE:-1}"

# Pin (version / URL / sha256 / size). Uplifted for an x86_64 Actions host.
RELEASE="20260812"
ASSET="x86_64-cross-tools-loongarch64-unknown-linux-gnu-latest.tar.xz"
TOOLCHAIN_URL="https://github.com/loong64/cross-tools/releases/download/${RELEASE}/${ASSET}"
TOOLCHAIN_SHA256="adf5a3235465f936eff045a4098c7acb445370b53413fcecb41938755b093f5c"
TOOLCHAIN_SIZE="87205240"
CACHE_DIR="$HOME/.rillml-loongarch-toolchain"

if [[ "$HOST_ARCH" != "x86_64" ]] && [[ "$HOST_ARCH" != "amd64" ]]; then
  echo "LoongArch gate expects an x86_64 Actions host; found ${HOST_ARCH}." >&2
  exit 1
fi

# ─────────────────────────────────────────────────────────────────────────
# 1. Obtain a pinned, checksum-verified LoongArch cross toolchain (cached).
# ─────────────────────────────────────────────────────────────────────────
mkdir -p "$CACHE_DIR"
ARCHIVE="$CACHE_DIR/${ASSET}"
if [[ ! -x "$(find "$CACHE_DIR" -type f -name 'loongarch64-unknown-linux-gnu-gcc' | head -n1)" ]]; then
  if [[ ! -f "$ARCHIVE" ]]; then
    echo "==> Downloading pinned LoongArch toolchain (release ${RELEASE}, stable)"
    echo "    ${TOOLCHAIN_URL}"
    curl -fL --retry 5 --retry-delay 2 -o "$ARCHIVE" "$TOOLCHAIN_URL"
  fi
  echo "==> Verifying sha256"
  echo "${TOOLCHAIN_SHA256}  ${ARCHIVE}" | sha256sum -c -
  echo "==> Verifying size"
  actual="$(stat -c %s "$ARCHIVE" 2>/dev/null || stat -f %z "$ARCHIVE")"
  if [[ "$actual" != "$TOOLCHAIN_SIZE" ]]; then
    echo "Size mismatch: expected ${TOOLCHAIN_SIZE}, got ${actual}" >&2
    exit 1
  fi
  echo "==> Extracting toolchain"
  tar -xf "$ARCHIVE" -C "$CACHE_DIR"
  rm -f "$ARCHIVE"
fi

CC="$(find "$CACHE_DIR" -type f -name 'loongarch64-unknown-linux-gnu-gcc' | head -n1)"
if [[ -z "$CC" ]]; then
  echo "Could not locate loongarch64-unknown-linux-gnu-gcc under ${CACHE_DIR}" >&2
  exit 1
fi
BINDIR="$(dirname "$CC")"
export PATH="${BINDIR}:$PATH"

# Resolve the target sysroot the way the compiler itself sees it. This is more
# reliable than guessing the crosstool-ng directory name.
SYSROOT="$("$CC" -print-sysroot)"
if [[ -z "$SYSROOT" || ! -d "$SYSROOT" ]]; then
  echo "Cross gcc did not report a usable sysroot (got '${SYSROOT}')." >&2
  exit 1
fi
echo "==> LoongArch gcc: ${CC}"
echo "==> LoongArch sysroot: ${SYSROOT}"
echo "==> Compiler version: $("$CC" -dumpversion)"

# Cross-compiling C dependencies (e.g. wasmtime's vm helpers) with the cross
# gcc needs an explicit target sysroot; otherwise the compiler falls back to
# the host /usr/include and fails on `bits/libc-header-start.h`. cc-rs reads
# the per-target `CFLAGS_<triple>` env var (dashes -> underscores).
export CFLAGS_loongarch64_unknown_linux_gnu="--sysroot=${SYSROOT}"
echo "==> Cross C sysroot: CFLAGS_loongarch64_unknown_linux_gnu=--sysroot=${SYSROOT}"

echo "==> Installing QEMU user-mode (qemu-loongarch64)"
sudo apt-get update
sudo apt-get install -y --no-install-recommends qemu-user python3
command -v qemu-loongarch64 >/dev/null || { echo "qemu-loongarch64 not available" >&2; exit 1; }

echo "==> Adding Rust target ${TARGET}"
rustup target add "$TARGET"

# QEMU user-mode proves insufficient to reach libgcc_s.so.1 / libstdc++.so.6 on
# its own: the loongarch loader's default search covers <sysroot>/lib64 and
# <sysroot>/usr/lib64 (where libc lives) but NOT <sysroot>/lib (where the
# toolchain puts libgcc_s.so.1 and libstdc++.so.6). Without them a Rust binary
# fails at load with `libgcc_s.so.1: cannot open shared object file` (exit 127).
# Inject an explicit library path into the guest through QEMU's -E.
RUNNER="qemu-loongarch64 -L ${SYSROOT} -E LD_LIBRARY_PATH=${SYSROOT}/lib:${SYSROOT}/lib64"

# Register a binfmt_misc handler for loongarch64 with the SAME sysroot + explicit
# library path as the cargo runner. A loongarch64 ELF that crosses the host
# kernel's execve boundary (post-release download-and-run re-verification) is
# then executed correctly through QEMU instead of failing with ENOEXEC.
if [[ -e /proc/sys/fs/binfmt_misc/register ]]; then
  ./scripts/register_qemu_binfmt.sh "$TARGET" "$SYSROOT" \
    -E "LD_LIBRARY_PATH=${SYSROOT}/lib:${SYSROOT}/lib64"
else
  echo "==> binfmt_misc unavailable; direct foreign exec will not work"
fi

# Configure Cargo: cross-link with the LoongArch gcc and execute every foreign
# binary through QEMU user-mode transparently.
mkdir -p .cargo
cat >> .cargo/config.toml <<EOF

# Added by scripts/loongarch-cross-gate.sh (Actions-first direct QEMU)
[target.${TARGET}]
linker = "${CC}"
runner = "${RUNNER}"
EOF
echo "==> QEMU runner: ${RUNNER}"

# ─────────────────────────────────────────────────────────────────────────
# 2. WASM handler fixtures (host-native wasm32 build) so the --all-features
#    tests perform a REAL LoongArch WASM handler load+invoke under QEMU via the
#    Wasmtime Pulley backend — not a "skipping" no-op.
# ─────────────────────────────────────────────────────────────────────────
echo "==> Building WASM handler fixtures (wasm32-unknown-unknown, host-native)"
rustup target add wasm32-unknown-unknown
mkdir -p target
( cd handlers/echo-handler && \
  cargo build --locked --release --target wasm32-unknown-unknown >/dev/null )
if ! command -v wasm-tools >/dev/null 2>&1; then
  cargo install wasm-tools --locked --version "1.254.0"
fi
wasm-tools component new \
  handlers/echo-handler/target/wasm32-unknown-unknown/release/echo-handler.wasm \
  -o target/echo-handler.wasm
export ECHO_HANDLER_WASM="${PWD}/target/echo-handler.wasm"

# ─────────────────────────────────────────────────────────────────────────
# 3. Real-execute the crate test suite on loongarch64 via QEMU.
# ─────────────────────────────────────────────────────────────────────────
# The runtime_process crate tests spawn `rill-runtime` as a child. Under QEMU a
# foreign child cannot exec through the host kernel (no binfmt_misc), and
# posix_spawn's error channel is broken by QEMU's CLONE_VFORK emulation. Inject
# the host-native QEMU interpreter so the test launches the child through it.
export RILL_RUNTIME_EXEC_PREFIX="$RUNNER"

echo "==> cargo test --workspace --target ${TARGET}"
cargo test --locked --workspace --all-targets --all-features \
  --exclude rill-ml-python --target "$TARGET"
echo "==> cargo test --doc --target ${TARGET}"
cargo test --locked --workspace --doc --all-features \
  --exclude rill-ml-python --target "$TARGET"

# ─────────────────────────────────────────────────────────────────────────
# 4. Runtime Stable gate: build the full default Runtime (with `wasm`, i.e. the
#    WASM/Pulley handler backend) and exercise it under QEMU.
# ─────────────────────────────────────────────────────────────────────────
if [[ "$RUNTIME_SMOKE" != "0" ]]; then
  echo "==> Runtime smoke for ${TARGET} via QEMU user-mode"
  cargo build --locked --release --target "$TARGET" \
    -p rill-runtime --bin rill-runtime --bin rill-pack --features wasm

  BIN="target/${TARGET}/release"
  run_qemu() {
    # shellcheck disable=SC2086
    $RUNNER "$BIN/$1" "${@:2}"
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

  echo '-- create and inspect signed WASM handler pack'
  RILL_SIGNING_KEY_HEX="$seed" run_qemu rill-pack create-handler \
    --manifest handlers/echo-handler/manifest.json \
    --module "$ECHO_HANDLER_WASM" \
    --output /tmp/echo.rillhandler
  run_qemu rill-pack inspect-handler \
    --handler /tmp/echo.rillhandler \
    --key-id "$key_id" \
    --public-key-hex "$pubkey" >/dev/null

  echo '-- handshake over IPC with signed WASM handler'
  REQUEST='{"method":"handshake","requestId":"loongarch-smoke","apiVersion":2,"clientName":"loongarch-smoke","clientVersion":"0.0.0"}'
  RESPONSE="$(printf '%s\n' "$REQUEST" | timeout 60 $RUNNER "$BIN/rill-runtime" serve \
    --pack /tmp/example.rillpack \
    --model-trust-key "$key_id=$pubkey" \
    --handler /tmp/echo.rillhandler \
    --handler-trust-key "$key_id=$pubkey")"
  echo "$RESPONSE" | grep -q '"kind":"handshake"'
  echo "$RESPONSE" | grep -q '"apiVersion":2'

  echo '-- runtime smoke PASSED'
fi

echo "==> LoongArch gate PASSED for ${TARGET}"
