#!/usr/bin/env bash
# RillML final-published-artifact QEMU re-verification — Actions-first, no Docker.
#
# Downloads the ACTUAL GitHub Release runtime asset for a Supported Linux target
# and re-verifies that exact binary (size/SHA-256 against the signed index,
# signed model, signed handler, WASM handler invoke, IPC) by executing it under
# DIRECT user-mode QEMU on the native Actions host — no target-arch container,
# no dependency on whether upstream rust:* images publish a manifest for the
# platform. This replaces the old Docker-manifest-conditional skip for
# RISC-V64 / LoongArch64 (release prompt §14/§15).
#
# The runtime being re-verified is always the published release asset downloaded
# from the GitHub Release; rill-pack is built from source only because it is the
# independent pack verifier (never a substitute for the published runtime).
#
# Principle: native runner > direct QEMU > VM > Docker.
#
# Usage:
#   ./scripts/post-release-qemu-verify.sh <triple> [host_smoke args...]
#
#   triple            riscv64gc-unknown-linux-gnu | loongarch64-unknown-linux-gnu
#   host_smoke args   passed through to smoke-test/host_smoke.py
#                     (--index-url --expected-version --expected-channel
#                      --runtime-id --rill-pack-bin --log ...)
#
# The script auto-injects --target-os linux, --target-arch, and --exec-prefix
# so host_smoke.py selects the foreign asset and executes it under QEMU.

set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="${1:?usage: post-release-qemu-verify.sh <triple> [host_smoke args...]}"
shift

case "$TARGET" in
  riscv64gc-unknown-linux-gnu)
    TARGET_ARCH="riscv64"
    SYSROOT="/usr/riscv64-linux-gnu"
    echo "==> Installing qemu-user + riscv64 libc sysroot"
    sudo apt-get update
    sudo apt-get install -y --no-install-recommends qemu-user python3 libc6-dev-riscv64-cross
    ;;
  loongarch64-unknown-linux-gnu)
    TARGET_ARCH="loongarch64"
    echo "==> Installing qemu-user + python3"
    sudo apt-get update
    sudo apt-get install -y --no-install-recommends qemu-user python3
    command -v qemu-loongarch64 >/dev/null || { echo "qemu-loongarch64 not available" >&2; exit 1; }
    # Pinned LoongArch cross-tools sysroot (same source of truth as
    # scripts/loongarch-cross-gate.sh) so the released binary's glibc matches.
    RELEASE="20260812"
    ASSET="x86_64-cross-tools-loongarch64-unknown-linux-gnu-latest.tar.xz"
    TOOLCHAIN_URL="https://github.com/loong64/cross-tools/releases/download/${RELEASE}/${ASSET}"
    TOOLCHAIN_SHA256="adf5a3235465f936eff045a4098c7acb445370b53413fcecb41938755b093f5c"
    CACHE_DIR="$HOME/.rillml-loongarch-toolchain"
    mkdir -p "$CACHE_DIR"
    ARCHIVE="$CACHE_DIR/${ASSET}"
    if [[ ! -x "$(find "$CACHE_DIR" -type f -name 'loongarch64-unknown-linux-gnu-gcc' | head -n1)" ]]; then
      if [[ ! -f "$ARCHIVE" ]]; then
        echo "==> Downloading pinned LoongArch toolchain (release ${RELEASE})"
        curl -fL --retry 5 --retry-delay 2 -o "$ARCHIVE" "$TOOLCHAIN_URL"
      fi
      echo "==> Verifying sha256"
      echo "${TOOLCHAIN_SHA256}  ${ARCHIVE}" | sha256sum -c -
      echo "==> Extracting toolchain"
      tar -xf "$ARCHIVE" -C "$CACHE_DIR"
      rm -f "$ARCHIVE"
    fi
    CC="$(find "$CACHE_DIR" -type f -name 'loongarch64-unknown-linux-gnu-gcc' | head -n1)"
    if [[ -z "$CC" ]]; then
      echo "Could not locate loongarch64-unknown-linux-gnu-gcc under ${CACHE_DIR}" >&2
      exit 1
    fi
    SYSROOT="$("$CC" -print-sysroot)"
    if [[ -z "$SYSROOT" || ! -d "$SYSROOT" ]]; then
      echo "loongarch cross gcc did not report a usable sysroot (got '${SYSROOT}')" >&2
      exit 1
    fi
    ;;
  *)
    echo "post-release-qemu-verify.sh: unsupported triple '$TARGET'" >&2
    exit 1
    ;;
esac

echo "==> ${TARGET} sysroot: ${SYSROOT}"

# Prefer the dynamically-linked qemu user binary, fall back to the static one.
if command -v "qemu-${TARGET_ARCH}" >/dev/null 2>&1; then
  QEMU="qemu-${TARGET_ARCH}"
elif command -v "qemu-${TARGET_ARCH}-static" >/dev/null 2>&1; then
  QEMU="qemu-${TARGET_ARCH}-static"
else
  echo "qemu-${TARGET_ARCH}(-static) not found on PATH" >&2
  exit 1
fi

# Assemble the QEMU user-mode invocation: sysroot plus per-target flags.
#   riscv64: -cpu rv64 (matches scripts/qemu-cross-gate.sh)
#   loongarch64: -E LD_LIBRARY_PATH=... so libgcc_s/libstdc++ are found
#                (matches scripts/loongarch-cross-gate.sh)
PREFIX_ARGS=("$QEMU" "-L" "$SYSROOT")
case "$TARGET" in
  riscv64gc-unknown-linux-gnu) PREFIX_ARGS+=("-cpu" "rv64") ;;
  loongarch64-unknown-linux-gnu) PREFIX_ARGS+=("-E" "LD_LIBRARY_PATH=${SYSROOT}/lib:${SYSROOT}/lib64") ;;
esac
EXEC_PREFIX="$(printf '%s ' "${PREFIX_ARGS[@]}")"
EXEC_PREFIX="${EXEC_PREFIX% }"

echo "==> Re-verifying published ${TARGET} release asset under direct QEMU"
echo "    exec-prefix: ${EXEC_PREFIX}"

python3 smoke-test/host_smoke.py \
  --target-os linux \
  --target-arch "$TARGET_ARCH" \
  --exec-prefix "$EXEC_PREFIX" \
  "$@"
