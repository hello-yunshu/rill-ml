#!/usr/bin/env bash
set -euo pipefail
: "${RILL_ZIG_TARGET:?RILL_ZIG_TARGET must name a Zig musl target}"

args=()
for arg in "$@"; do
  case "$arg" in
    # Rust passes its target triple to the C linker. The wrapper has already
    # selected the canonical Zig target; passing the Rust spelling again makes
    # Zig reject armv7/i686 as unknown architectures.
    --target=*) continue ;;
    # Let Zig provide one musl CRT and libc. Rust's self-contained target
    # arguments would otherwise either duplicate _start or hide Zig's sysroot.
    -nostartfiles|-nodefaultlibs|-Wl,-melf_i386) continue ;;
    */crt1.o|*/crti.o|*/crtbegin.o|*/crtend.o|*/crtn.o) continue ;;
  esac
  args+=("$arg")
done

exec zig cc -target "$RILL_ZIG_TARGET" "${args[@]}"
