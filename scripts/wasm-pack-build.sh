#!/usr/bin/env bash
# RillML WASM bindings build + test (host-native).
#
# Builds crates/rill-ml-wasm into a publishable npm layout:
#   pkg/       -> wasm-pack --target web     (ESM, for browsers/bundlers)
#   pkg-node/  -> wasm-pack --target nodejs  (CJS, for Node.js)
# and runs the wasm-bindgen test suite under Node (`wasm-pack test --node`).
#
# Usage:
#   ./scripts/wasm-pack-build.sh
#
# docker-first note: on hosts with Docker, prefer scripts/docker-wasm-build.sh
# for the canonical gate. This script is the native/CI fallback and the
# developer path on hosts without a working Docker setup.

set -euo pipefail

cd "$(dirname "$0")/.."

# Ensure rustup-managed tools are on PATH (non-interactive shells, Docker).
export PATH="/usr/local/cargo/bin:$PATH"

echo "==> Ensuring wasm32-unknown-unknown target is installed"
rustup target add wasm32-unknown-unknown

if ! wasm-pack --version >/dev/null 2>&1; then
  echo "==> wasm-pack not found; installing (compatible range ~0.15.0)"
  cargo install wasm-pack --locked --version "~0.15.0"
else
  echo "==> Using existing wasm-pack: $(wasm-pack --version)"
fi

cd crates/rill-ml-wasm

echo "==> Building web (ESM) build -> pkg/"
wasm-pack build --release --target web --out-dir pkg

echo "==> Building node (CJS) build -> pkg-node/"
wasm-pack build --release --target nodejs --out-dir pkg-node

# npm reads ignore files in every subdirectory of the package. wasm-pack writes
# a `.gitignore` containing `*` inside each output dir so its artifacts stay out
# of git — but that also excludes them from the published npm tarball (defeating
# the `files` whitelist). An empty `.npmignore` in each output dir overrides the
# subdirectory `.gitignore` per npm's documented precedence, so the artifacts
# remain packable while still ignored by git.
for dir in pkg pkg-node; do
  : > "$dir/.npmignore"
done

echo "==> Running wasm-bindgen tests under Node (wasm-pack test --node --release)"
wasm-pack test --node --release

cd ../..

echo "==> WASM build + test PASSED"
echo "    pkg/       -> browser ESM build"
echo "    pkg-node/  -> Node CJS build"
