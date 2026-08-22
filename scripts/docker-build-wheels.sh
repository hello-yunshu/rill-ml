#!/usr/bin/env bash
# RillML Docker-first manylinux wheel build for the Python bindings.
#
# Builds stable-ABI (abi3, CPython 3.9+) wheels for Linux x86_64
# (linux/amd64) and aarch64 (linux/arm64) inside the pinned
# ghcr.io/pyo3/maturin image. The whole repository is mounted at /io because
# crates/rill-ml-python depends on rill-ml via a path dependency, so the
# container must see the full workspace. Wheels are collected into dist/.
#
# The produced wheels are manylinux abi3 wheels (tagged cp39-abi3) and work
# on any CPython 3.9+ for the matching Linux architecture. This is the
# Docker-first way to produce Linux artifacts: manylinux wheels are only
# portable when built inside the manylinux-compatible maturin image, so the
# build MUST run in Docker (never on the native host).
#
# Usage:
#   ./scripts/docker-build-wheels.sh                           # amd64 + arm64
#   TARGETS="linux/amd64" ./scripts/docker-build-wheels.sh     # one platform
#   MATURIN_IMAGE=ghcr.io/pyo3/maturin:v1.14.1 \
#     ./scripts/docker-build-wheels.sh                         # explicit image
#
# docker-first principle: Linux wheel builds MUST be scripted through Docker.

set -euo pipefail

cd "$(dirname "$0")/.."

# Pin a concrete release tag (never `latest`/`main`) for reproducibility.
# v1.14.1 is a real ghcr.io/pyo3/maturin release tag and matches
# scripts/requirements-dev.txt (maturin==1.14.1). Override via MATURIN_IMAGE.
MATURIN_IMAGE="${MATURIN_IMAGE:-ghcr.io/pyo3/maturin:v1.14.1}"
# Space-separated Docker platforms to build, overridable via TARGETS.
TARGETS="${TARGETS:-linux/amd64 linux/arm64}"

echo "==> maturin image: ${MATURIN_IMAGE}"
mkdir -p dist

for plat in ${TARGETS}; do
  echo "==> Building manylinux wheel for ${plat}"
  # The image entrypoint is already `maturin`, so the command starts with the
  # `build` subcommand (no leading `maturin`).
  docker run --rm \
    --platform "${plat}" \
    -v "$PWD":/io \
    -w /io \
    "${MATURIN_IMAGE}" \
    build --release --manifest-path crates/rill-ml-python/Cargo.toml --out /io/dist
done

echo "==> Wheels produced under dist/:"
count=0
for wheel in dist/*.whl; do
  [ -e "${wheel}" ] || continue
  count=$((count + 1))
  echo "  ${wheel}"
done
if [ "${count}" -eq 0 ]; then
  echo "ERROR: no .whl files were produced under dist/ (see build logs above)" >&2
  exit 1
fi

echo "==> Build complete: ${count} wheel(s) in dist/"
