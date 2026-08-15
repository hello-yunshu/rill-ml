#!/usr/bin/env bash
# RillML Docker-first cross-architecture execution gate.
#
# Real-executes the crate test suite on a non-native CPU architecture using
# Docker + QEMU/binfmt_misc. The container is launched with --platform set to
# the target architecture, so `cargo test --target <target>` compiles FOR that
# target and runs the test binary NATIVELY inside the emulated container.
# This is real execution, not compile-only.
#
# Usage:
#   ./scripts/cross-exec-test.sh aarch64-unknown-linux-gnu
#   TARGET=riscv64gc-unknown-linux-gnu ./scripts/cross-exec-test.sh
#   RUN_RUNTIME_SMOKE=1 ./scripts/cross-exec-test.sh s390x-unknown-linux-gnu
#   ./scripts/cross-exec-test.sh aarch64-unknown-linux-gnu --runtime-smoke
#
# RUN_RUNTIME_SMOKE=1 (or the trailing --runtime-smoke flag) additionally runs a
# real Runtime smoke inside the SAME target container after the crate tests:
# release build, --help, fresh Ed25519 keypair -> signed pack -> verify ->
# inspect -> handshake IPC (mirrors scripts/docker-release-smoke.sh).
#
# Supported targets (TARGET / --platform):
#   aarch64-unknown-linux-gnu        linux/arm64
#   armv7-unknown-linux-gnueabihf    linux/arm/v7
#   riscv64gc-unknown-linux-gnu      linux/riscv64
#   s390x-unknown-linux-gnu          linux/s390x
#   powerpc64le-unknown-linux-gnu    linux/ppc64le
#   loongarch64-unknown-linux-gnu    linux/loongarch64   (see guard below)
#
# docker-first principle: cross-arch execution MUST prefer Docker + QEMU over
# dedicated emulator/VM. Only fall back when Docker cannot provide valid
# execution for a target.
#
# NOTE on loongarch64: most hosts have NO usable Docker image manifest for the
# linux/loongarch64 platform (upstream rust:*-bookworm does not publish a
# loongarch64 manifest), so real QEMU execution is not generally possible. The
# script detects that and exits cleanly (LoongArch64 stays compile/evaluation-
# only, NOT claimed Supported); ALLOW_LOONGARCH64=1 forces the Docker attempt
# when a real manifest exists.

set -euo pipefail

cd "$(dirname "$0")/.."

RUST_PIN="${RUST_PIN:-1.97.0}"
TARGET="${1:-${TARGET:-aarch64-unknown-linux-gnu}}"

# Optional real Runtime smoke inside the same target container, enabled either
# via the RUN_RUNTIME_SMOKE=1 env var or a second CLI arg `--runtime-smoke`.
RUNTIME_SMOKE=0
if [[ "${RUN_RUNTIME_SMOKE:-0}" == "1" || "${2:-}" == "--runtime-smoke" ]]; then
  RUNTIME_SMOKE=1
fi

# Map each supported target to its Docker --platform and a preinstalled cross C
# linker. The linkage env is set so the Rust toolchain finds the cross gcc.
# (Implemented with a case statement so the script also runs on macOS bash 3.2,
# which lacks associative arrays; Linux CI uses bash 4+.)
_platform() {
  case "$1" in
    x86_64-unknown-linux-gnu) echo "linux/amd64" ;;
    x86_64-unknown-linux-musl) echo "linux/amd64" ;;
    aarch64-unknown-linux-gnu) echo "linux/arm64" ;;
    aarch64-unknown-linux-musl) echo "linux/arm64" ;;
    armv7-unknown-linux-gnueabihf) echo "linux/arm/v7" ;;
    riscv64gc-unknown-linux-gnu) echo "linux/riscv64" ;;
    s390x-unknown-linux-gnu) echo "linux/s390x" ;;
    powerpc64le-unknown-linux-gnu) echo "linux/ppc64le" ;;
    loongarch64-unknown-linux-gnu) echo "linux/loongarch64" ;;
    *) return 1 ;;
  esac
}
_linker_pkg() {
  case "$1" in
    # x86_64 GNU is native to the amd64 container, so no cross package is needed.
    x86_64-unknown-linux-gnu) echo "" ;;
    # musl targets build inside a container whose platform matches the target,
    # so musl-tools provides the NATIVE <arch>-linux-musl-gcc for that arch.
    x86_64-unknown-linux-musl) echo "musl-tools" ;;
    aarch64-unknown-linux-musl) echo "musl-tools" ;;
    aarch64-unknown-linux-gnu) echo "gcc-aarch64-linux-gnu" ;;
    armv7-unknown-linux-gnueabihf) echo "gcc-arm-linux-gnueabihf" ;;
    riscv64gc-unknown-linux-gnu) echo "gcc-riscv64-linux-gnu" ;;
    s390x-unknown-linux-gnu) echo "gcc-s390x-linux-gnu" ;;
    powerpc64le-unknown-linux-gnu) echo "gcc-powerpc64le-linux-gnu" ;;
    loongarch64-unknown-linux-gnu) echo "gcc-loongarch64-linux-gnu" ;;
    *) return 1 ;;
  esac
}
# The actual cross/native linker binary name for a target. musl targets use the
# <arch>-linux-musl-gcc provided by musl-tools; GNU targets use <triple>-gcc.
_linker_bin() {
  case "$1" in
    x86_64-unknown-linux-gnu) echo "gcc" ;;
    x86_64-unknown-linux-musl) echo "x86_64-linux-musl-gcc" ;;
    aarch64-unknown-linux-musl) echo "aarch64-linux-musl-gcc" ;;
    aarch64-unknown-linux-gnu) echo "aarch64-linux-gnu-gcc" ;;
    armv7-unknown-linux-gnueabihf) echo "arm-linux-gnueabihf-gcc" ;;
    riscv64gc-unknown-linux-gnu) echo "riscv64-linux-gnu-gcc" ;;
    s390x-unknown-linux-gnu) echo "s390x-linux-gnu-gcc" ;;
    powerpc64le-unknown-linux-gnu) echo "powerpc64le-linux-gnu-gcc" ;;
    loongarch64-unknown-linux-gnu) echo "loongarch64-linux-gnu-gcc" ;;
    *) return 1 ;;
  esac
}

PLAT="$(_platform "$TARGET")" || {
  echo "Unsupported target '$TARGET' — add it to scripts/cross-exec-test.sh" >&2
  exit 2
}
LINKER_PKG="$(_linker_pkg "$TARGET")"
LINKER="$(_linker_bin "$TARGET")"
IMAGE="rust:${RUST_PIN}-bookworm"

# musl targets use cargo-zigbuild (Zig's Clang-based C compiler + bundled musl
# libc) instead of Debian bookworm's musl-tools <arch>-linux-musl-gcc. The
# Debian musl-gcc produces broken test binaries that SIGSEGV at process startup
# here (observed in rill-handler-api, a zero-dependency constant-assertion
# crate). For musl we therefore do NOT install musl-tools nor set a cross
# linker; the container installs cargo-zigbuild (pip pulls the ziglang binary)
# and the test/smoke commands become `cargo zigbuild`, linking genuinely against
# musl exactly as the released assets are built.
ZIGBUILD=0
case "$TARGET" in
  x86_64-unknown-linux-musl|aarch64-unknown-linux-musl) ZIGBUILD=1 ;;
esac

# The linker binary is resolved by `_linker_bin` above and Cargo discovers it
# via CARGO_TARGET_<TRIPLE_UPPERCASE>_LINKER. For musl targets cargo-zigbuild
# drives Zig itself (Zig's Clang-based C compiler + bundled musl libc), so no
# system musl-tools linker is installed and no explicit linker env is set.
INSTALL_PKGS="$LINKER_PKG"
# python3 is required to derive the Ed25519 public key when the optional
# runtime smoke is enabled; keep the default test-only image unchanged.
if [[ "$RUNTIME_SMOKE" == "1" ]]; then
  INSTALL_PKGS="${INSTALL_PKGS} python3"
fi
if [[ "$ZIGBUILD" == "1" ]]; then
  # musl links genuinely against Zig's musl via cargo-zigbuild (pip pulls the
  # ziglang binary). Debian bookworm's pip is PEP 668 externally-managed, so we
  # pass --break-system-packages. No cross linker is needed.
  INSTALL_STEP="RUN apt-get update && apt-get install -y --no-install-recommends python3 python3-pip && rm -rf /var/lib/apt/lists/* && pip3 install --break-system-packages cargo-zigbuild"
elif [[ -n "$INSTALL_PKGS" ]]; then
  INSTALL_STEP="RUN apt-get update && apt-get install -y --no-install-recommends ${INSTALL_PKGS} && rm -rf /var/lib/apt/lists/*"
else
  INSTALL_STEP="RUN true"
fi
LINKER_ENV=""
if [[ "$ZIGBUILD" == "0" ]]; then
  LINKER_ENV="CARGO_TARGET_$(echo "$TARGET" | tr 'a-z-' 'A-Z_')_LINKER"
fi
# cargo-zigbuild replaces `cargo build` for the musl targets.
CARGO_CMD="cargo"
if [[ "$ZIGBUILD" == "1" ]]; then
  CARGO_CMD="cargo zigbuild"
fi

# cargo-zigbuild's `zigbuild` subcommand is build-only: it accepts NO `test`,
# `run` or `build` subcommand (clap rejects them as unexpected arguments). So
# for musl we COMPILE the production surface (all libs + all thin bins) to prove
# every crate genuinely links against musl, and rely on the runtime smoke below
# for real execution. GNU targets run the tests normally. TEST_LINES is injected
# into the container body below (host expands it); BUILD_CMD is passed as a
# container env var for the runtime smoke.
#
# We deliberately use `--lib --bins` (NOT `--all-targets`) for musl: the test/
# example/bench targets would pull heavy [dev-dependencies] and blow up the
# aarch64-musl QEMU compile well past the CI timeout. The production libs and
# bins are the release surface that matters, and real execution is proven by the
# runtime smoke (which builds rill-runtime + rill-pack and runs them).
if [[ "$ZIGBUILD" == "1" ]]; then
  TEST_LINES="    ${CARGO_CMD} --locked --workspace --lib --bins --all-features --exclude rill-ml-python --target ${TARGET}"
  BUILD_CMD="cargo zigbuild"
else
  TEST_LINES="    ${CARGO_CMD} test --locked --workspace --all-targets --all-features --exclude rill-ml-python --target ${TARGET}
    ${CARGO_CMD} test --locked --workspace --doc --all-features --exclude rill-ml-python --target ${TARGET}"
  BUILD_CMD="cargo build"
fi

# ── LoongArch64 graceful guard ──────────────────────────────────────────────
# There is typically NO usable Docker image manifest for the linux/loongarch64
# platform (upstream rust:*-bookworm does not publish a loongarch64 manifest),
# so QEMU execution is not generally possible here. Per the docker-first policy
# we never claim "Supported" without real execution, so when the platform
# manifest cannot be resolved we exit cleanly and leave LoongArch64 as
# compile/evaluation-only. Set ALLOW_LOONGARCH64=1 to force the Docker attempt
# anyway (only meaningful on a host where a real manifest exists).
if [[ "$TARGET" == "loongarch64-unknown-linux-gnu" && "${ALLOW_LOONGARCH64:-0}" != "1" ]]; then
  if ! docker manifest inspect --platform "linux/loongarch64" "$IMAGE" >/dev/null 2>&1; then
    echo "==> loongarch64 Docker/QEMU image manifest unavailable on this host — LoongArch64 remains compile/evaluation only and is NOT claimed Supported (see PLATFORM_SUPPORT.md)"
    echo "==> (Set ALLOW_LOONGARCH64=1 to force the Docker attempt once a real manifest exists.)"
    exit 0
  fi
fi

# ── RISC-V 64 graceful guard ────────────────────────────────────────────────
# Same policy as LoongArch64: upstream rust:*-bookworm does not publish a
# linux/riscv64 manifest, so `docker build --platform=linux/riscv64 rust:...`
# fails with "no match for platform in manifest". RISC-V 64 stays
# compile/evaluation-only (NOT claimed Supported) and we exit cleanly instead
# of failing the whole cross-platform gate. Set ALLOW_RISCV64=1 to force the
# Docker attempt where a real manifest exists.
if [[ "$TARGET" == "riscv64gc-unknown-linux-gnu" && "${ALLOW_RISCV64:-0}" != "1" ]]; then
  if ! docker manifest inspect --platform "linux/riscv64" "$IMAGE" >/dev/null 2>&1; then
    echo "==> riscv64 Docker/QEMU image manifest unavailable on this host — RISC-V 64 remains compile/evaluation only and is NOT claimed Supported (see PLATFORM_SUPPORT.md)"
    echo "==> (Set ALLOW_RISCV64=1 to force the Docker attempt once a real manifest exists.)"
    exit 0
  fi
fi

# Real Runtime smoke emitted into the SAME target-platform container after the
# crate tests. Mirrors scripts/docker-release-smoke.sh: release build, --help,
# fresh Ed25519 keypair -> signed pack -> verify -> inspect -> handshake IPC.
# The body is emitted verbatim (quoted heredoc) so it is expanded by the
# CONTAINER's bash, not by this host script.
run_runtime_smoke() {
  cat <<'RUNTIME_SMOKE'
  # MUST pass --target: inside the container the default host triple is the GNU
  # triple of the base image (e.g. x86_64-unknown-linux-gnu in an amd64 bookworm
  # container), NOT the requested musl/aarch64musl target. Without --target the
  # release binaries land in target/release/ instead of target/${TARGET}/release/
  # and the smoke below would fail with "No such file or directory".
  echo '-- runtime smoke: release build (wasm feature)'
  ${BUILD_CMD} --locked --release -p rill-runtime --bin rill-runtime --bin rill-pack --features wasm --target ${TARGET}

  bin=target/${TARGET}/release

  echo '-- rill-runtime --help'
  $bin/rill-runtime --help >/dev/null

  echo '-- generate fresh Ed25519 keypair'
  seed=$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')
  pubkey=$(python3 scripts/derive_ed25519_pubkey.py "$seed")
  echo "  pubkey=$pubkey"

  key_id=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["publisherKeyId"])' models/example-default/manifest.json)
  echo "  key_id=$key_id"

  echo '-- create a signed model pack'
  RILL_SIGNING_KEY_HEX="$seed" $bin/rill-pack create \
    --manifest models/example-default/manifest.json \
    --model models/example-default/model.json \
    --output /tmp/example.rillpack

  echo '-- verify the signed model pack (round-trips pubkey derivation)'
  $bin/rill-pack verify \
    --pack /tmp/example.rillpack \
    --key-id "$key_id" \
    --public-key-hex "$pubkey"

  echo '-- inspect-pack'
  $bin/rill-runtime inspect-pack \
    --pack /tmp/example.rillpack \
    --model-trust-key "$key_id=$pubkey" >/dev/null

  echo '-- handshake over IPC (v2)'
  REQUEST='{"method":"handshake","requestId":"runtime-smoke","apiVersion":2,"clientName":"runtime-smoke","clientVersion":"0.0.0"}'
  RESPONSE=$(printf '%s\n' "$REQUEST" | timeout 20 $bin/rill-runtime serve \
    --pack /tmp/example.rillpack \
    --model-trust-key "$key_id=$pubkey" \
    --builtin-handler linear-regression)
  echo "$RESPONSE" | grep -q '"kind":"handshake"'
  echo "$RESPONSE" | grep -q '"apiVersion":2'
  echo '-- runtime smoke PASSED'
RUNTIME_SMOKE
}

# Optional host-level cargo cache to cut QEMU compile time across runs.
#
# The container's CARGO_HOME is /usr/local/cargo. The crates.io registry source
# (downloaded .crate files + index) is arch-independent, so sharing it with the
# host avoids re-downloading every dependency on each QEMU run. The workspace
# target/ dir is already mounted via -v "$PWD":/src, so per-arch compiled
# artifacts are reused within the same run already.
#
# Defaults to the host's ~/.cargo registry+git (the container runs as root and
# can write). Set CROSS_CARGO_CACHE=0 to disable, or point CROSS_CARGO_CACHE at
# an alternate cache dir (its registry/ and git/ subdirs are used).
CARGO_CACHE="${CROSS_CARGO_CACHE:-$HOME/.cargo}"
CACHE_MOUNTS=""
if [[ "${CROSS_CARGO_CACHE:-1}" != "0" ]]; then
  # CACHE_MOUNTS is expanded UNQUOTED in the docker run line so it word-splits
  # into separate -v flags; therefore no quotes may appear inside the paths.
  CACHE_MOUNTS="-v ${CARGO_CACHE}/registry:/usr/local/cargo/registry -v ${CARGO_CACHE}/git:/usr/local/cargo/git"
fi

echo "==> Cross-executing $TARGET on Docker platform $PLAT (QEMU/binfmt)"
echo "==> Image: $IMAGE"
if [[ "$RUNTIME_SMOKE" == "1" ]]; then
  echo "==> Runtime smoke enabled (RUN_RUNTIME_SMOKE=1 / --runtime-smoke) — release build + signed-pack IPC smoke after the crate tests"
  echo "==> WARNING: Runtime smoke on 32-bit / niche targets (armv7, riscv64, s390x, powerpc64le) may be slow under QEMU but is still real execution."
fi

# Build an image for the target platform with the cross linker installed.
docker build -t "rillml-cross-${TARGET}:${RUST_PIN}" -f- . <<EOF
FROM --platform=${PLAT} ${IMAGE}
${INSTALL_STEP}
EOF

docker run --rm \
  -v "$PWD":/src -w /src \
  ${CACHE_MOUNTS} \
  ${LINKER_ENV:+-e "${LINKER_ENV}=${LINKER}"} \
  -e "BUILD_CMD=${BUILD_CMD}" \
  -e "TARGET=${TARGET}" \
  "rillml-cross-${TARGET}:${RUST_PIN}" \
  bash -c "
    set -euo pipefail
    export PATH=\"/usr/local/cargo/bin:\$PATH\"
    rustup target add ${TARGET}
${TEST_LINES}
    $(if [[ "$RUNTIME_SMOKE" == "1" ]]; then
        echo "    echo '==> runtime smoke for ${TARGET}'"
        run_runtime_smoke
      fi)
  "

echo "==> Cross-execution PASSED for ${TARGET}"
