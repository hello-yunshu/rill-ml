#!/usr/bin/env bash
# Build and statically qualify the Full Runtime for official OpenHarmony/HarmonyOS
# ARM64. The SDK is supplied by CI or the caller; it is never committed.

set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="${OHOS_TARGET:-aarch64-unknown-linux-ohos}"
OUTPUT="${1:-dist/rill-runtime-ohos-aarch64}"
if [[ "$TARGET" != "aarch64-unknown-linux-ohos" ]]; then
  echo "unsupported OHOS target: $TARGET" >&2
  exit 2
fi

native="${OHOS_SDK_NATIVE:-}"
if [[ -z "$native" && -n "${OHOS_NDK_HOME:-}" ]]; then
  if [[ -x "$OHOS_NDK_HOME/llvm/bin/clang" ]]; then
    native="$OHOS_NDK_HOME"
  else
    native="$OHOS_NDK_HOME/native"
  fi
fi
if [[ -z "$native" || ! -x "$native/llvm/bin/clang" || ! -x "$native/llvm/bin/llvm-ar" || ! -d "$native/sysroot" ]]; then
  echo "official OHOS native SDK not found; set OHOS_SDK_NATIVE or OHOS_NDK_HOME" >&2
  exit 2
fi

rustup target add "$TARGET"

cfg="$(rustc --print cfg --target "$TARGET")"
for expected in 'target_arch="aarch64"' 'target_env="ohos"' 'target_os="linux"' 'target_pointer_width="64"' 'target_endian="little"'; do
  if ! grep -Fxq "$expected" <<<"$cfg"; then
    echo "rustc cfg assertion failed: missing $expected" >&2
    exit 1
  fi
done

sdk_version="unknown"
if [[ -f "$native/oh-uni-package.json" ]]; then
  sdk_version="$(python3 - "$native/oh-uni-package.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as stream:
    document = json.load(stream)
print(document.get("version", "unknown"))
PY
)"
fi
echo "OHOS SDK native path: $native"
echo "OHOS SDK version: $sdk_version"
echo "Rust target: $TARGET"

linker_dir="$(mktemp -d "${TMPDIR:-/tmp}/rill-ohos-linker.XXXXXX")"
trap 'rm -rf "$linker_dir"' EXIT
linker="$linker_dir/aarch64-unknown-linux-ohos-clang"
cat >"$linker" <<EOF
#!/bin/sh
exec "$native/llvm/bin/clang" -target aarch64-linux-ohos --sysroot="$native/sysroot" -D__MUSL__ "\$@"
EOF
chmod +x "$linker"

export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_OHOS_LINKER="$linker"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_OHOS_AR="$native/llvm/bin/llvm-ar"

mkdir -p "$(dirname "$OUTPUT")"
cargo build --locked --release --target "$TARGET" -p rill-runtime --bin rill-runtime
cp "target/$TARGET/release/rill-runtime" "$OUTPUT"

"$(dirname "$0")/ohos-release-verify.sh" "$OUTPUT"
echo "OHOS Full Runtime cross-build PASS: $OUTPUT"
