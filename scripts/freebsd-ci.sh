#!/usr/bin/env bash
# RillML FreeBSD native CI entry point (Phase 7: x86_64-unknown-freebsd).
#
# This is the NATIVE FreeBSD verification path. Docker + QEMU cannot substitute
# the FreeBSD kernel, so cross-architecture container execution is impossible
# for this OS; per the platform-support policy, real execution-level
# verification for FreeBSD must run on a real FreeBSD host (a FreeBSD VM,
# dedicated CI, or Cirrus CI). This script establishes that native path:
#
#   1. installs the Rust toolchain via rustup (if missing),
#   2. cargo fmt --check and clippy -D warnings (guarded on component presence),
#   3. the full workspace test suite (all targets + doc tests),
#   4. a Runtime smoke: release build, --help, fresh Ed25519 keypair ->
#      signed pack -> verify -> inspect -> handshake IPC.
#
# The toolchain targets the host triple x86_64-unknown-freebsd natively; the
# `rustup target add x86_64-unknown-freebsd` below is idempotent / a no-op on a
# FreeBSD host and is kept explicit for clarity.
#
# If Rust is NOT preinstalled and rustup is unavailable, install Rust on the
# FreeBSD host first (e.g. `pkg install rust`, or the rustup-init binary) and
# re-run; the script then exits with a clear message.
#
# Usage:
#   ./scripts/freebsd-ci.sh
#
# Exit non-zero on any step failure.

set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> RillML FreeBSD native CI"

# ── 1. Toolchain ────────────────────────────────────────────────────────────
if ! command -v cargo >/dev/null 2>&1; then
  if command -v rustup >/dev/null 2>&1; then
    echo "==> cargo missing — installing Rust stable via rustup (minimal profile)"
    rustup toolchain install stable --profile minimal
    rustup default stable
  else
    echo "!! cargo not found and rustup is not available on this FreeBSD host." >&2
    echo "!! Install Rust first (e.g. pkg install rust, or the rustup-init binary) and re-run." >&2
    exit 1
  fi
fi
# Host target add: idempotent, and a no-op on a FreeBSD host (the default host
# triple is already x86_64-unknown-freebsd). Kept explicit for clarity.
if command -v rustup >/dev/null 2>&1; then
  rustup target add x86_64-unknown-freebsd
fi

# ── 2. Static checks (guarded on component presence) ────────────────────────
if command -v cargo-fmt >/dev/null 2>&1; then
  echo "==> cargo fmt --all --check"
  cargo fmt --all --check
else
  echo "!! rustfmt (cargo-fmt) not found — skipping fmt check"
fi

if command -v cargo-clippy >/dev/null 2>&1; then
  echo "==> cargo clippy --locked --workspace --all-targets --all-features ( -D warnings)"
  cargo clippy --locked --workspace --all-targets --all-features --exclude rill-ml-python -- -D warnings
else
  echo "!! clippy (cargo-clippy) not found — skipping clippy"
fi

# ── 3. Full workspace tests ─────────────────────────────────────────────────
echo "==> cargo test (all targets / all features)"
cargo test --locked --workspace --all-targets --all-features --exclude rill-ml-python

echo "==> cargo test (doc tests)"
cargo test --locked --workspace --doc --all-features --exclude rill-ml-python

# ── 4. Runtime smoke (mirrors scripts/docker-release-smoke.sh, native) ──────
echo "==> Runtime smoke"
cargo build --locked --release -p rill-runtime --bin rill-runtime --bin rill-pack --features wasm

bin=target/release

echo '-- rill-runtime --help'
"$bin/rill-runtime" --help >/dev/null

# python3 is needed to derive the Ed25519 public key from a fresh seed.
if command -v python3 >/dev/null 2>&1; then
  echo '-- generate fresh Ed25519 keypair'
  seed=$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')
  pubkey=$(python3 scripts/derive_ed25519_pubkey.py "$seed")
  echo "  pubkey=$pubkey"

  key_id=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["publisherKeyId"])' models/example-default/manifest.json)
  echo "  key_id=$key_id"

  echo '-- create a signed model pack'
  RILL_SIGNING_KEY_HEX="$seed" "$bin/rill-pack" create \
    --manifest models/example-default/manifest.json \
    --model models/example-default/model.json \
    --output /tmp/example.rillpack

  echo '-- verify the signed model pack (round-trips pubkey derivation)'
  "$bin/rill-pack" verify \
    --pack /tmp/example.rillpack \
    --key-id "$key_id" \
    --public-key-hex "$pubkey"

  echo '-- inspect-pack'
  "$bin/rill-runtime" inspect-pack \
    --pack /tmp/example.rillpack \
    --model-trust-key "$key_id=$pubkey" >/dev/null

  echo '-- handshake over IPC (v2)'
  REQUEST='{"method":"handshake","requestId":"freebsd-ci","apiVersion":2,"clientName":"freebsd-ci","clientVersion":"0.0.0"}'
  RESPONSE=$(printf '%s\n' "$REQUEST" | timeout 20 "$bin/rill-runtime" serve \
    --pack /tmp/example.rillpack \
    --model-trust-key "$key_id=$pubkey" \
    --builtin-handler linear-regression)
  echo "$RESPONSE" | grep -q '"kind":"handshake"'
  echo "$RESPONSE" | grep -q '"apiVersion":2'
  echo '-- runtime smoke PASSED (fresh-key signed pack + handshake)'
else
  echo "!! python3 not found — cannot derive the Ed25519 public key from a fresh seed"
  committed_key=""
  if [[ -f .github/workflows/pipeline.yml ]]; then
    committed_key=$(sed -n 's/^[[:space:]]*RILL_PUBLIC_KEY_HEX:[[:space:]]*\([0-9a-fA-F]\{64\}\).*/\1/p' .github/workflows/pipeline.yml | head -n 1)
  fi
  if [[ -n "$committed_key" ]]; then
    echo "!! Falling back to the repository's committed public key (RILL_PUBLIC_KEY_HEX) from .github/workflows/pipeline.yml"
    echo "!! The repo does not commit a pre-signed .rillpack and there is no way to derive a"
    echo "!! matching secret from the fresh seed, so the signed-pack section is skipped"
    echo "!! (signature verification skipped)."
  else
    echo "!! No committed public key found in .github/workflows/pipeline.yml"
    echo "!! signed-pack section skipped (signature verification skipped)."
  fi
  echo "!! handshake IPC skipped (serve requires a signature-verified pack)"
  echo '-- runtime smoke PARTIAL: release build + --help passed; signed-pack verification skipped'
fi

echo "==> FreeBSD CI PASSED"
