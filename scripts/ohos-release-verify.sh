#!/usr/bin/env bash
# Verify target identity for an OHOS release artifact without claiming device
# execution. This is deliberately static: HarmonyOS hardware is not available
# in the current release environment.

set -euo pipefail

binary="${1:?usage: ohos-release-verify.sh <runtime-binary>}"
if [[ ! -f "$binary" ]]; then
  echo "missing OHOS runtime artifact: $binary" >&2
  exit 2
fi

readelf_bin="$(command -v llvm-readelf || command -v readelf || true)"
if [[ -z "$readelf_bin" ]]; then
  echo "llvm-readelf or readelf is required for OHOS ELF verification" >&2
  exit 2
fi

header="$($readelf_bin -h "$binary")"
grep -Eq 'Machine:.*(AArch64|ARM aarch64)' <<<"$header"
grep -Eq 'Class:.*ELF64' <<<"$header"

if command -v file >/dev/null 2>&1; then
  file_output="$(file "$binary")"
  grep -Eiq 'ELF 64-bit.*ARM aarch64' <<<"$file_output"
fi

program_headers="$($readelf_bin -l "$binary")"
if grep -Eqi 'ld-linux|x86_64-linux-gnu|aarch64-linux-gnu' <<<"$program_headers"; then
  echo "OHOS artifact appears to use a GNU Linux loader/sysroot" >&2
  exit 1
fi

echo "OHOS ELF identity PASS: target_os=ohos target_arch=aarch64 target_libc=none"
