#!/usr/bin/env bash
set -euo pipefail
: "${RILL_ZIG_TARGET:?RILL_ZIG_TARGET must name a Zig musl target}"

args=()
for arg in "$@"; do
  # Rust's i686 musl self-contained linker adds this GNU ld emulation flag.
  # Zig already selects the correct emulation from -target and rejects the
  # explicit spelling, so remove only that target-specific flag.
  if [[ "$RILL_ZIG_TARGET" == "x86-linux-musl" && "$arg" == "-Wl,-melf_i386" ]]; then
    continue
  fi
  args+=("$arg")
done

# Rust supplies its own self-contained musl CRT objects. Prevent Zig from
# injecting a second startup sequence, which otherwise duplicates _start on
# armv7 and other non-native targets.
exec zig cc -target "$RILL_ZIG_TARGET" -nostdlib "${args[@]}"
