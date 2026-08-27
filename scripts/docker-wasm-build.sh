#!/usr/bin/env bash
# RillML Docker-first WASM bindings gate.
#
# Builds + tests crates/rill-ml-wasm inside a pinned `rust:<pin>-bookworm`
# container (matching Dockerfile.test), then reuses the native build script
# (scripts/wasm-pack-build.sh) so Docker and the host execute the identical
# steps:
#   - rustup target add wasm32-unknown-unknown
#   - wasm-pack build --release --target web    -> pkg/
#   - wasm-pack build --release --target nodejs -> pkg-node/
#   - wasm-pack test --node --release
#
# docker-first principle: builds/tests that can run in Docker MUST run in
# Docker. This is the canonical gate for the WASM bindings; CI wires the same
# script.
#
# Usage:
#   ./scripts/docker-wasm-build.sh                   # pinned 1.97.0
#   RUST_PIN=1.97.0 ./scripts/docker-wasm-build.sh   # explicit toolchain pin

set -euo pipefail

cd "$(dirname "$0")/.."

RUST_PIN="${RUST_PIN:-1.97.0}"
IMAGE="rust:${RUST_PIN}-bookworm"
TAG="rillml-wasm:${RUST_PIN}"

# Node.js LTS (22.x) pinned for the wasm-bindgen test runner. Debian bookworm's
# apt nodejs is v18, which is EOL and CRASHES V8 during wasm-bindgen test
# bootstrap (a native abort while loading the generated test runner), so we
# install a modern pinned LTS from the official tarballs instead.
NODE_VERSION="${NODE_VERSION:-22.23.2}"

# wasm-pack downloads PREBUILT wasm-bindgen binaries that are linked against a
# newer glibc than bookworm ships (notably wasm-bindgen-test-runner on
# linux-aarch64 requires GLIBC_2.38/2.39 while bookworm has 2.36), so they fail
# to execute in the container. Build the matching tools from source into the
# image instead: wasm-pack itself and wasm-bindgen-cli at the version pinned by
# Cargo.lock. wasm-pack then reuses them from PATH rather than downloading
# incompatible prebuilt binaries.
WB_VERSION="$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | grep '^version' | head -n1 | sed 's/^version = "\(.*\)"/\1/')"
if [[ -z "$WB_VERSION" ]]; then
  echo "error: could not read the pinned wasm-bindgen version from Cargo.lock" >&2
  exit 1
fi
echo "==> Pinned wasm-bindgen-cli version: ${WB_VERSION} (Node.js ${NODE_VERSION})"

echo "==> Preparing Docker image ${TAG} from ${IMAGE}"
# wasm-pack test --node drives the wasm-bindgen test suite through Node.js,
# which the base rust image does not ship. We install the pinned Node LTS from
# the official tarball (per-architecture naming: arm64 / x64) and put it on
# PATH ahead of the system dirs.
docker build -t "${TAG}" -f- . <<EOF
FROM ${IMAGE}
ARG NODE_VERSION=${NODE_VERSION}
RUN apt-get update && apt-get install -y --no-install-recommends curl xz-utils \\
    && ARCH="\$(uname -m)" \\
    && case "\$ARCH" in \\
         aarch64) NODE_ARCH=arm64 ;; \\
         x86_64)  NODE_ARCH=x64 ;; \\
         *)       echo "unsupported architecture: \$ARCH" >&2; exit 1 ;; \\
       esac \\
    && curl -fsSL -o /tmp/node.tar.xz "https://nodejs.org/dist/v\${NODE_VERSION}/node-v\${NODE_VERSION}-linux-\${NODE_ARCH}.tar.xz" \\
    && tar -xJf /tmp/node.tar.xz -C /opt \\
    && ln -s "/opt/node-v\${NODE_VERSION}-linux-\${NODE_ARCH}/bin/node" /usr/local/bin/node \\
    && ln -s "/opt/node-v\${NODE_VERSION}-linux-\${NODE_ARCH}/bin/npm" /usr/local/bin/npm \\
    && rm /tmp/node.tar.xz \\
    && apt-get purge -y curl xz-utils \\
    && apt-get autoremove -y \\
    && rm -rf /var/lib/apt/lists/* \\
    && node --version
RUN cargo install wasm-pack --locked --version "~0.15.0" \\
    && cargo install wasm-bindgen-cli --version "=${WB_VERSION}"
EOF

echo "==> Running WASM build + test inside ${TAG}"
docker run --rm -v "$PWD":/src -w /src "${TAG}" bash -c '
  set -euo pipefail
  export PATH="/usr/local/cargo/bin:$PATH"
  bash scripts/wasm-pack-build.sh
  echo "==> Node smoke test (pkg-node consumer path, pinned Node LTS)"
  cd crates/rill-ml-wasm
  node tests/node-smoke.mjs
'

echo "==> Docker WASM build + test PASSED"
