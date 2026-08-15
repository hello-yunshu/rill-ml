#!/usr/bin/env bash
# RillML Docker-first Android static-library build for the rill-ml-ffi C ABI.
#
# docker-first principle: Android has NO locally installed NDK on the host,
# so all Android cross-compilation MUST run inside a pinned Docker image
# (linux/amd64 is fine — this is a cross build from macOS). The image pins a
# concrete Rust toolchain (rust:1.97.0-bookworm) and a pinned Android NDK
# (android-ndk-r27d, downloaded from the official Google repository).
#
# What it produces (into dist-android/, gitignored):
#   librill_ml_ffi-{aarch64,x86_64}-linux-android.{a,so}   staticlib + cdylib
#   librill_ml_jni-{aarch64,x86_64}-linux-android.so       JNI shim linked
#                                                          against the staticlib
# Each artifact is architecture-verified with `file` (and archive-member
# inspection for the .a staticlibs). The JNI shim is additionally compiled and
# linked against the freshly built staticlib to prove the JNI layer is valid
# (jni.h comes from the container's JDK).
#
# Usage:
#   ./scripts/docker-build-android.sh                          # both ABIs
#   NDK_VERSION=android-ndk-r27d ./scripts/docker-build-android.sh
#   RUST_PIN=1.97.0 ./scripts/docker-build-android.sh
#
# Requires: Docker. Run from anywhere; the repo root is resolved from the
# script location. POSIX-bash (macOS bash 3.2 compatible: no associative
# arrays / mapfile).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

RUST_PIN="${RUST_PIN:-1.97.0}"
# Pinned Android NDK (third-party toolchain — exempt from the project version
# rule). Release tarball names are android-ndk-<release>-linux.zip on the
# official Google repository.
NDK_VERSION="${NDK_VERSION:-android-ndk-r27d}"
IMAGE="rust:${RUST_PIN}-bookworm"
IMAGE_TAG="rillml-android:${RUST_PIN}-${NDK_VERSION}"
DIST="${DIST_ANDROID:-$ROOT/dist-android}"

echo "==> RillML Android cross-build via Docker (docker-first)"
echo "==> Image: ${IMAGE}   NDK: ${NDK_VERSION}   Rust: ${RUST_PIN}"
echo "==> Output dir: ${DIST}"

# Build (and cache) the toolchain image: pinned Rust + pinned NDK + a JDK
# (for jni.h, used to compile/link the JNI shim smoke).
if docker image inspect "${IMAGE_TAG}" >/dev/null 2>&1; then
    echo "==> Reusing cached toolchain image ${IMAGE_TAG}"
else
    echo "==> Building toolchain image ${IMAGE_TAG} (downloads the Android NDK once)"
    docker build \
        --build-arg RUST_PIN="$RUST_PIN" \
        --build-arg NDK_VERSION="$NDK_VERSION" \
        -t "${IMAGE_TAG}" \
        -f- . <<'DOCKERFILE'
ARG RUST_PIN
FROM --platform=linux/amd64 rust:${RUST_PIN}-bookworm

ARG NDK_VERSION
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        python3 unzip curl ca-certificates openjdk-17-jdk-headless \
    && rm -rf /var/lib/apt/lists/*

# Pinned Android NDK from the official Google repository.
RUN curl -fsSL -o /tmp/ndk.zip \
        "https://dl.google.com/android/repository/${NDK_VERSION}-linux.zip" \
    && unzip -q /tmp/ndk.zip -d /opt \
    && rm /tmp/ndk.zip \
    && mv "/opt/${NDK_VERSION}" /opt/android-ndk

ENV ANDROID_NDK_HOME=/opt/android-ndk
DOCKERFILE
fi

mkdir -p "$DIST"

echo "==> Cross-compiling rill-ml-ffi (aarch64-linux-android + x86_64-linux-android)"
# The repo is mounted at /src so cargo can resolve the workspace path deps.
# CARGO_TARGET_DIR=/src/target-android keeps the host `target/` untouched;
# the container removes it (and chowns dist-android) before exiting.
#
# NOTE: no `--platform linux/amd64` here. The toolchain image is a
# single-architecture amd64 build, so it is selected automatically on any
# host. Adding `--platform` makes some daemons (e.g. OrbStack) resolve the
# name through the registry instead of using the local image, which breaks
# the build. On an amd64 host (Linux CI) the container runs natively; on
# Apple Silicon it runs under Rosetta/QEMU emulation, which is all this
# cross build needs.
docker run --rm \
    -v "$ROOT":/src -w /src \
    -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
    -e CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=/opt/android-ndk/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android24-clang \
    -e CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER=/opt/android-ndk/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android24-clang \
    -e CARGO_TARGET_DIR=/src/target-android \
    "${IMAGE_TAG}" \
    bash -c '
        set -euo pipefail
        trap "rm -rf /src/target-android" EXIT
        export PATH="/usr/local/cargo/bin:$PATH"

        rustup target add aarch64-linux-android x86_64-linux-android

        cargo build --locked --release -p rill-ml-ffi --target aarch64-linux-android
        cargo build --locked --release -p rill-ml-ffi --target x86_64-linux-android

        echo "==> Collecting artifacts into /src/dist-android"
        mkdir -p /src/dist-android
        cp /src/target-android/aarch64-linux-android/release/librill_ml_ffi.a  /src/dist-android/librill_ml_ffi-aarch64-linux-android.a
        cp /src/target-android/aarch64-linux-android/release/librill_ml_ffi.so /src/dist-android/librill_ml_ffi-aarch64-linux-android.so
        cp /src/target-android/x86_64-linux-android/release/librill_ml_ffi.a   /src/dist-android/librill_ml_ffi-x86_64-linux-android.a
        cp /src/target-android/x86_64-linux-android/release/librill_ml_ffi.so  /src/dist-android/librill_ml_ffi-x86_64-linux-android.so

        echo "==> Compiling + linking the JNI shim against the staticlib"
        NDK_BIN=/opt/android-ndk/toolchains/llvm/prebuilt/linux-x86_64/bin
        JAVA_HOME_DIR="$(dirname "$(dirname "$(readlink -f "$(command -v javac)")")")"

        "$NDK_BIN/aarch64-linux-android24-clang" -c \
            -I"$JAVA_HOME_DIR/include" -I"$JAVA_HOME_DIR/include/linux" \
            -I/src/crates/rill-ml-ffi/include \
            /src/crates/rill-ml-ffi/android/jni/rill_ml_jni.c \
            -o /src/target-android/rill_ml_jni-aarch64.o
        "$NDK_BIN/aarch64-linux-android24-clang" -shared \
            /src/target-android/rill_ml_jni-aarch64.o \
            /src/dist-android/librill_ml_ffi-aarch64-linux-android.a \
            -ldl -llog -lm -lz \
            -o /src/dist-android/librill_ml_jni-aarch64-linux-android.so

        "$NDK_BIN/x86_64-linux-android24-clang" -c \
            -I"$JAVA_HOME_DIR/include" -I"$JAVA_HOME_DIR/include/linux" \
            -I/src/crates/rill-ml-ffi/include \
            /src/crates/rill-ml-ffi/android/jni/rill_ml_jni.c \
            -o /src/target-android/rill_ml_jni-x86_64.o
        "$NDK_BIN/x86_64-linux-android24-clang" -shared \
            /src/target-android/rill_ml_jni-x86_64.o \
            /src/dist-android/librill_ml_ffi-x86_64-linux-android.a \
            -ldl -llog -lm -lz \
            -o /src/dist-android/librill_ml_jni-x86_64-linux-android.so

        chown -R "${HOST_UID}:${HOST_GID}" /src/dist-android
        echo "==> container build finished"
    '

echo "==> Verifying architectures with file"
failures=0

# verify_file <path> <expected-substring>: shared objects report ELF arch
# directly; staticlib archives are handled by verify_archive_arch.
verify_file() {
    local path="$1" expect="$2" info
    info="$(file "$path")"
    if printf '%s' "$info" | grep -q "$expect"; then
        echo "  ok   ${path}"
        echo "         ${info}"
    else
        echo "  FAIL ${path}"
        echo "         ${info}"
        failures=$((failures + 1))
    fi
}

# verify_archive_arch <path> <expected-substring>: extracts a member of an ar
# archive and checks its ELF architecture.
#
# The archive is produced by LLVM/GNU ar (Rust staticlibs). macOS ships BSD ar,
# which cannot read the GNU `//` long-name string table, so parsing is done with
# a small Python helper that is portable across macOS/Linux hosts. python3 is
# available on both the host and inside the container.
verify_archive_arch() {
    local path="$1" expect="$2" work member info
    work="$(mktemp -d)"
    member="$work/member.o"
    # Extract the first non-special .o member of a (possibly GNU long-name) ar
    # archive. Mirrors `ar -x` but works with macOS BSD ar, which cannot read
    # the GNU `//` long-name string table emitted by LLVM/GNU ar. python3 is
    # available on both macOS and Linux hosts.
    if python3 - "$path" "$member" <<'PY' 2>/dev/null; then
import sys

def extract(archive_path, out_path):
    with open(archive_path, "rb") as f:
        if f.read(8) != b"!<arch>\n":
            return False
        string_table = b""
        while True:
            header = f.read(60)
            if len(header) < 60:
                break
            name_raw = header[:16].decode("latin-1").rstrip()
            size = int(header[48:58].decode("latin-1").strip() or "0")
            body = f.read(size)
            if len(body) < size:
                break
            if size % 2 == 1:
                f.read(1)
            if name_raw == "//":
                string_table = body
                continue
            if name_raw in ("/", "/SYM64/", "__.SYMDEF", "__.SYMDEF SORTED"):
                continue
            if name_raw.startswith("/"):
                try:
                    idx = int(name_raw[1:])
                    end = string_table.find(b"/\n", idx)
                    if end == -1:
                        end = len(string_table)
                    name = string_table[idx:end].decode("latin-1")
                except Exception:
                    name = "member"
            else:
                name = name_raw
            if name.endswith(".o"):
                with open(out_path, "wb") as out:
                    out.write(body)
                return True
    return False

if not extract(sys.argv[1], sys.argv[2]):
    sys.exit(1)
PY
        info="$(file "$member")"
        rm -rf "$work"
        if printf '%s' "$info" | grep -q "$expect"; then
            echo "  ok   ${path}  (archive member: $(basename "$member"))"
            echo "         ${info}"
        else
            echo "  FAIL ${path}  (archive member: ${info})"
            failures=$((failures + 1))
        fi
    else
        echo "  FAIL ${path} (could not parse archive)"
        failures=$((failures + 1))
        rm -rf "$work"
    fi
}

verify_file         "$DIST/librill_ml_ffi-aarch64-linux-android.so" "ELF 64-bit LSB.*ARM aarch64"
verify_file         "$DIST/librill_ml_ffi-x86_64-linux-android.so"  "ELF 64-bit LSB.*x86-64"
verify_archive_arch "$DIST/librill_ml_ffi-aarch64-linux-android.a"  "ARM aarch64"
verify_archive_arch "$DIST/librill_ml_ffi-x86_64-linux-android.a"   "x86-64"
verify_file         "$DIST/librill_ml_jni-aarch64-linux-android.so" "ELF 64-bit LSB.*ARM aarch64"
verify_file         "$DIST/librill_ml_jni-x86_64-linux-android.so"  "ELF 64-bit LSB.*x86-64"

if [ "$failures" -ne 0 ]; then
    echo "==> Android build FAILED: ${failures} verification(s) failed" >&2
    exit 1
fi

echo "==> Android build PASSED"
ls -la "$DIST"
