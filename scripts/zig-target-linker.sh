#!/usr/bin/env bash
set -euo pipefail
: "${RILL_ZIG_TARGET:?RILL_ZIG_TARGET must name a Zig musl target}"
exec zig cc -target "$RILL_ZIG_TARGET" "$@"
