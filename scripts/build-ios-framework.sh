#!/usr/bin/env bash
# RillML iOS XCFramework build — native macOS/Xcode toolchain.
#
# docker-first applies to Android (scripts/docker-build-android.sh); iOS is
# built natively with the macOS/Xcode toolchain (there is no iOS Docker image).
# This script:
#   1. cross-compiles the rill-ml-ffi Rust staticlib for the iOS device and
#      simulator targets,
#   2. cross-compiles the Swift wrapper (crates/rill-ml-ffi/ios) per slice with
#      swiftc, using -enable-library-evolution so the framework carries a
#      .swiftinterface (required by `xcodebuild -create-xcframework` in newer
#      Xcode versions),
#   3. links each slice into a self-contained static framework and assembles
#      the final XCFrameworks with `xcodebuild -create-xcframework`:
#
#        dist-ios/RillMl.xcframework    Swift wrapper (RillMl module)
#        dist-ios/CRillMl.xcframework   C ABI module (CRillMl module map + header)
#
#   A consumer adds BOTH xcframeworks: `import RillMl` resolves the Swift module
#   from RillMl.framework, and RillMl's module (binary + swiftinterface)
#   references `CRillMl`, which is resolved from CRillMl.framework's module
#   map + rill_ml.h.
#
# Usage:
#   ./scripts/build-ios-framework.sh
#
# Requires: macOS + Xcode (swiftc/swift build/xcodebuild), cargo, and the rust
# targets aarch64-apple-ios + aarch64-apple-ios-sim (rustup target add ...).
# The simulator slice is arm64-only (Apple Silicon); add x86_64 sim support by
# installing the x86_64-apple-ios rust target and extending the SLICES list.
#
# Run from anywhere; the repo root is resolved from the script location.
# POSIX-bash (macOS bash 3.2 compatible: no associative arrays / mapfile).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IOS_DIR="$ROOT/crates/rill-ml-ffi/ios"
DIST="${DIST_IOS:-$ROOT/dist-ios}"
CONFIG="${CONFIG:-release}"

# Slice table: rust-target | swift-triple | sdk-name | xcframework-slide-id
SLICES=(
    "aarch64-apple-ios arm64-apple-ios iphoneos ios-arm64"
    "aarch64-apple-ios-sim arm64-apple-ios-simulator iphonesimulator ios-arm64-simulator"
)

echo "==> RillML iOS XCFramework build (native Xcode toolchain)"
echo "==> Config: ${CONFIG}   Rust targets: $(printf '%s\n' "${SLICES[@]}" | awk '{print $1}' | tr '\n' ' ')"
echo "==> Output dir: ${DIST}"

command -v xcodebuild >/dev/null 2>&1 || { echo "error: xcodebuild not found (Xcode required)" >&2; exit 1; }
command -v cargo >/dev/null 2>&1 || { echo "error: cargo not found" >&2; exit 1; }
command -v swiftc >/dev/null 2>&1 || { echo "error: swiftc not found" >&2; exit 1; }

rm -rf "$DIST"
mkdir -p "$DIST"

# ---------------------------------------------------------------------------
# Per-slice: rust staticlib + swift wrapper -> static frameworks
# ---------------------------------------------------------------------------
for slice in "${SLICES[@]}"; do
    set -- $slice
    rust_target="$1"
    swift_triple="$2"
    sdk_name="$3"
    slide_id="$4"
    arch="${swift_triple%%-*}" # arm64
    cargo_dir="$ROOT/target/${rust_target}/${CONFIG}"
    stage="$DIST/stage/${slide_id}"
    fw="$stage/RillMl.framework"
    cfw="$stage/CRillMl.framework"

    echo "==> [${rust_target}] building rill-ml-ffi Rust staticlib"
    (cd "$ROOT" && cargo build --locked --release -p rill-ml-ffi --target "$rust_target")

    rm -rf "$stage"
    mkdir -p "$fw/Modules/RillMl.swiftmodule" "$cfw/Headers" "$cfw/Modules"

    echo "==> [${swift_triple}] compiling Swift wrapper with swiftc (library evolution)"
    SDK_PATH="$(xcrun --sdk "$sdk_name" --show-sdk-path)"
    swiftc \
        -target "$swift_triple" -sdk "$SDK_PATH" \
        -swift-version 5 -module-name RillMl \
        -enable-library-evolution -parse-as-library -O \
        -emit-library -static -o "$stage/libRillMl.a" \
        -emit-module \
        -emit-module-path "$fw/Modules/RillMl.swiftmodule/${arch}.swiftmodule" \
        -emit-module-interface-path "$fw/Modules/RillMl.swiftmodule/${arch}.swiftinterface" \
        -I "$IOS_DIR/Sources/CRillMl/include" \
        "$IOS_DIR/Sources/RillMl/RillMl.swift" \
        "$IOS_DIR/Sources/RillMl/Mean.swift" \
        "$IOS_DIR/Sources/RillMl/LinearRegression.swift"

    # --- RillMl.framework: Swift lib + Rust staticlib merged into one archive
    libtool -static -o "$fw/RillMl" "$stage/libRillMl.a" "$cargo_dir/librill_ml_ffi.a"

    # --- CRillMl.framework: C ABI module (module map + header) for consumers
    cc -c -target "$swift_triple" -isysroot "$SDK_PATH" \
        "$IOS_DIR/Sources/CRillMl/dummy.c" -o "$stage/dummy.o"
    libtool -static -o "$cfw/CRillMl" "$stage/dummy.o"
    cp "$ROOT/crates/rill-ml-ffi/include/rill_ml.h" "$cfw/Headers/rill_ml.h"
    cat > "$cfw/Modules/module.modulemap" <<'MM'
framework module CRillMl {
    header "rill_ml.h"
    export *
}
MM

    # --- shared Info.plist for both frameworks
    for name in RillMl CRillMl; do
        plist_fw="$stage/${name}.framework"
        sed -e "s/__BUNDLE__/${name}/g" <<'PLIST' > "$plist_fw/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>__BUNDLE__</string>
  <key>CFBundleIdentifier</key><string>ai.rillml.__BUNDLE__</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>__BUNDLE__</string>
  <key>CFBundlePackageType</key><string>FMWK</string>
  <key>CFBundleShortVersionString</key><string>0.15.0</string>
  <key>CFBundleVersion</key><string>1</string>
</dict>
</plist>
PLIST
    done

    rm -f "$stage/libRillMl.a" "$stage/dummy.o"
    echo "==> [${slide_id}] framework slices assembled"
done

# ---------------------------------------------------------------------------
# XCFramework assembly
# ---------------------------------------------------------------------------
echo "==> Assembling dist-ios/RillMl.xcframework"
rm -rf "$DIST/RillMl.xcframework"
xcodebuild -quiet -create-xcframework \
    -framework "$DIST/stage/ios-arm64/RillMl.framework" \
    -framework "$DIST/stage/ios-arm64-simulator/RillMl.framework" \
    -output "$DIST/RillMl.xcframework"

echo "==> Assembling dist-ios/CRillMl.xcframework"
rm -rf "$DIST/CRillMl.xcframework"
xcodebuild -quiet -create-xcframework \
    -framework "$DIST/stage/ios-arm64/CRillMl.framework" \
    -framework "$DIST/stage/ios-arm64-simulator/CRillMl.framework" \
    -output "$DIST/CRillMl.xcframework"

# The per-slice frameworks are merged into the xcframeworks; keep only the
# final artifacts (drop the staging dir).
rm -rf "$DIST/stage"

echo "==> Verifying xcframework structure"
failures=0
for xc in "$DIST/RillMl.xcframework" "$DIST/CRillMl.xcframework"; do
    if [ -f "$xc/Info.plist" ]; then
        echo "  ok   $xc"
    else
        echo "  FAIL $xc (missing Info.plist)"
        failures=$((failures + 1))
    fi
    for slice in "$xc"/ios-*; do
        # shellcheck disable=SC2012
        fw="$(ls -d "$slice"/*.framework 2>/dev/null | head -n 1 || true)"
        if [ -n "$fw" ] && [ -d "$fw" ]; then
            echo "         slice: $(basename "$slice") -> $(basename "$fw")"
        else
            echo "  FAIL $xc slice missing framework: $slice"
            failures=$((failures + 1))
        fi
    done
done

# Verify the framework binaries carry the Rust symbols on both platforms.
echo "==> Verifying rill_ml_* symbols are linked into both device + sim slices"
for arch in ios-arm64 ios-arm64-simulator; do
    for name in RillMl CRillMl; do
        bin="$DIST/$name.xcframework/$arch/$name.framework/$name"
        if [ ! -f "$bin" ]; then
            echo "  FAIL missing binary: $bin"
            failures=$((failures + 1))
            continue
        fi
        if [ "$name" = "RillMl" ]; then
            # nm may exit non-zero on some Rust-produced members (LLVM version
            # skew with Xcode's nm) while still printing all symbols; ignore
            # its status and just count the defined rill_ml_* exports.
            syms="$(nm -gU "$bin" 2>/dev/null | grep -c 'T _rill_ml_' || true)"
            if [ "$syms" -gt 0 ]; then
                echo "  ok   $bin ($syms exported rill_ml_* symbols)"
            else
                echo "  FAIL $bin (no exported rill_ml_* symbols)"
                failures=$((failures + 1))
            fi
        fi
        printf '         %-8s %s: ' "$name" "$arch"
        file -b "$bin"
    done
done

if [ "$failures" -ne 0 ]; then
    echo "==> iOS build FAILED: ${failures} verification(s) failed" >&2
    exit 1
fi

echo "==> iOS build PASSED"
find "$DIST" -maxdepth 3 -type d | sort
