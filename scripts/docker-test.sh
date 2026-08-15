#!/usr/bin/env bash
# RillML Docker-first native test gate.
#
# Runs the full quality gate (fmt, clippy, test, doc) inside the pinned
# `rillml-test` image on the host architecture (x86_64 GNU by default).
#
# Usage:
#   ./scripts/docker-test.sh                        # native x86_64 GNU
#   RUST_PIN=1.97.0 ./scripts/docker-test.sh        # explicit toolchain pin
#
# docker-first principle: builds/tests that can run in Docker MUST run in
# Docker. This is the single entry point for the native Linux gate.

set -euo pipefail

cd "$(dirname "$0")/.."

RUST_PIN="${RUST_PIN:-1.97.0}"
IMAGE="rillml-test:${RUST_PIN}"

echo "==> Building test image ${IMAGE} (Rust ${RUST_PIN})"
docker build -t "${IMAGE}" -f Dockerfile.test \
  --build-arg RUST_PIN="${RUST_PIN}" .

echo "==> Running full quality gate in ${IMAGE}"
docker run --rm -v "$PWD":/src -w /src "${IMAGE}"

echo "==> Native Docker gate PASSED"