#!/usr/bin/env bash
# Compile and run the C smoke test for the rill-ml-ffi opaque-handle ABI.
#
# Builds the release cdylib, compiles crates/rill-ml-ffi/c_examples/basic.c
# against include/rill_ml.h and the generated library, then runs it. Fails
# (non-zero exit) if any C check fails.
#
# Requires: cargo, a C compiler (cc), and a POSIX shell.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FFI_DIR="$ROOT/crates/rill-ml-ffi"
LIBDIR="$ROOT/target/release"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/rill-ml-ffi-smoke.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

echo "==> Building rill-ml-ffi (release cdylib)"
(cd "$ROOT" && cargo build --release --locked -p rill-ml-ffi)

# The cdylib artifact name on each platform (cargo mangles '-' to '_').
case "$(uname -s)" in
    Darwin)
        LIB_FILE="librill_ml_ffi.dylib"
        RPATH="-Wl,-rpath,$LIBDIR"
        ;;
    Linux)
        LIB_FILE="librill_ml_ffi.so"
        RPATH="-Wl,-rpath,$LIBDIR"
        ;;
    *)
        echo "unsupported platform for C smoke test: $(uname -s)" >&2
        exit 2
        ;;
esac

if [ ! -f "$LIBDIR/$LIB_FILE" ]; then
    echo "error: expected $LIBDIR/$LIB_FILE not found" >&2
    exit 1
fi

echo "==> Compiling crates/rill-ml-ffi/c_examples/basic.c"
cc -Wall -Wextra -Werror \
    -I "$FFI_DIR/include" \
    -o "$WORK/rill_ml_basic" \
    "$FFI_DIR/c_examples/basic.c" \
    -L "$LIBDIR" -lrill_ml_ffi \
    $RPATH

echo "==> Running C smoke test"
"$WORK/rill_ml_basic"

echo "==> C smoke test passed"
