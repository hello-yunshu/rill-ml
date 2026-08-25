#!/usr/bin/env bash
set -euo pipefail
: "${RILL_ZIG_TARGET:?RILL_ZIG_TARGET must name a Zig musl target}"

args=()
for arg in "$@"; do
  # cc-rs adds the Rust target spelling. The wrapper's canonical Zig target
  # must win because Zig does not accept armv7/i686 as target architectures.
  [[ "$arg" == --target=* ]] && continue
  args+=("$arg")
done

exec zig cc -target "$RILL_ZIG_TARGET" "${args[@]}"
