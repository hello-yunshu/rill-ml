#!/usr/bin/env bash
# RillML QEMU binfmt_misc registration — Actions-first, no Docker.
#
# Registers a binfmt_misc handler so that a *foreign* ELF (riscv64 / loongarch64
# / aarch64 / armv7 / s390x / powerpc64le) reached through the host kernel's
# execve is transparently executed by QEMU user-mode with the *same* emulation
# config used by Cargo's `runner` (-L sysroot, -cpu, -E LD_LIBRARY_PATH).
#
# What needs binfmt and what does not:
#   * A crate test binary started by Cargo uses the `runner`, so it and any
#     child it spawns via execve stay inside the SAME QEMU user-mode process
#     tree (QEMU re-execs itself for the child). That path does NOT need
#     binfmt — it already inherits the emulation config through the process
#     tree / environment.
#   * A foreign binary launched from a NATIVE host process (e.g. the post-release
#     re-verification step, which downloads a riscv64/loongarch64 artifact and
#     executes it directly on the x86_64 runner) DOES cross the host kernel boundary.
#     Without a registered handler execve fails (ENOEXEC) / the loader is wrong.
#   * A QEMU-guest that execs a binary whose interpreter binfmt must assemble
#     also lands here as a belt-and-suspenders guarantee.
#
# Registering the correct handler up-front means the SAME download-and-run smoke
# works identically for every architecture, so release artifact re-verification
# has no per-arch special-casing.
#
# Usage:
#   ./scripts/register_qemu_binfmt.sh <triple> <sysroot> [extra qemu args...]
#   ./scripts/register_qemu_binfmt.sh riscv64gc-unknown-linux-gnu /usr/riscv64-linux-gnu
#   ./scripts/register_qemu_binfmt.sh loongarch64-unknown-linux-gnu \
#       <sysroot> -E LD_LIBRARY_PATH=<sysroot>/lib:<sysroot>/lib64
#
# The <triple> drives the ELF e_machine detection *warning* (see below); the
# qemu binary + interpreter flags are derived from it.

set -euo pipefail

TARGET="${1:-${1:?usage: register_qemu_binfmt.sh <triple> <sysroot> [extra qemu args...]}}"
SYSROOT="${2:-${2:?usage: register_qemu_binfmt.sh <triple> <sysroot> [extra qemu args...]}}"
shift 2 || true
EXTRA_ARGS=("$@")

# binfmt_misc must be mounted writable before we can register a handler.
if [[ ! -e /proc/sys/fs/binfmt_misc/register ]]; then
  sudo mount -t binfmt_misc none /proc/sys/fs/binfmt_misc 2>/dev/null || true
fi
if [[ ! -e /proc/sys/fs/binfmt_misc/register ]]; then
  echo "binfmt_misc is not available on this host; QEMU direct-exec won't work." >&2
  return 0 2>/dev/null || exit 0
fi

# ──────────────────────────────────────────────────────────────────────────────
# Per-target identity: qemu user-mode binary name + ELF header fingerprint.
#
# The magic/mask fingerprint matches the first 20 bytes of the ELF header:
#   0-3  magic "\x7fELF"
#   4    EI_CLASS    0x02 = 64-bit, 0x01 = 32-bit
#   5    EI_DATA     0x01 = little-endian, 0x00 = big-endian
#   6-8  EI_VERSION/OSABI/ABIVERSION   (masked off)
#   9-15 padding                         (masked off)
#   16-17  e_type   mask 0xfe -> matches ET_EXEC(2) and ET_DYN(3)
#   18-19  e_machine (arch-specific)
#
# Single mask for the 64-bit LE targets, with e_machine varying; s390x is
# big-endian and armv7 is 32-bit, so they get their own class/data bytes.
_qemu_bin() {
  case "$1" in
    riscv64gc-unknown-linux-gnu)        echo qemu-riscv64 ;;
    aarch64-unknown-linux-gnu)          echo qemu-aarch64 ;;
    armv7-unknown-linux-gnueabihf|arm*) echo qemu-arm ;;
    s390x-unknown-linux-gnu)            echo qemu-s390x ;;
    powerpc64le-unknown-linux-gnu)      echo qemu-ppc64le ;;
    loongarch64-unknown-linux-gnu)      echo qemu-loongarch64 ;;
    *) echo "register_qemu_binfmt.sh: unsupported triple '$1'" >&2; return 1 ;;
  esac
}

# magic/mask are stored escaped and expanded with printf %b before the write.
MASKMASK='\xff\xff\xff\xff\xff\xff\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\xfe\xff\xff\xff'
case "$TARGET" in
  riscv64gc-unknown-linux-gnu)
    MAGIC='\x7fELF\x02\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\xf3\x00'
    DEFAULT_CPU=' -cpu rv64' ;;
  aarch64-unknown-linux-gnu)
    MAGIC='\x7fELF\x02\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\xb7\x00'
    DEFAULT_CPU='' ;;
  armv7-unknown-linux-gnueabihf)
    MAGIC='\x7fELF\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x28\x00'
    DEFAULT_CPU='' ;;
  s390x-unknown-linux-gnu)
    MAGIC='\x7fELF\x02\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x00\x16'
    DEFAULT_CPU='' ;;
  powerpc64le-unknown-linux-gnu)
    MAGIC='\x7fELF\x02\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x15\x00'
    DEFAULT_CPU='' ;;
  loongarch64-unknown-linux-gnu)
    # EM_LOONGARCH = 258 = 0x0102 (little-endian two-byte e_machine).
    MAGIC='\x7fELF\x02\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x02\x01'
    DEFAULT_CPU='' ;;
  *) echo "register_qemu_binfmt.sh: no ELF fingerprint for '$TARGET'" >&2; return 1 ;;
esac

QEMU="$(_qemu_bin "$TARGET")"
if ! command -v "$QEMU" >/dev/null 2>&1; then
  # Some distros ship only the static variant; accept it.
  QEMU="${QEMU}-static"
  if ! command -v "$QEMU" >/dev/null 2>&1; then
    echo "register_qemu_binfmt.sh: could not locate '$QEMU' on PATH" >&2
    return 1 2>/dev/null || exit 1
  fi
fi
QEMU_PATH="$(command -v "$QEMU")"

# Interpreter gets the sysroot (-L) and any caller-supplied emulation flags.
INTERP="${QEMU_PATH} -L ${SYSROOT}${DEFAULT_CPU}"
for a in "${EXTRA_ARGS[@]:-}"; do
  INTERP+=" ${a}"
done

# Registration name must be unique and stable under binfmt_misc.
NAME="qemu-${TARGET%%-*}"
REG="/proc/sys/fs/binfmt_misc/${NAME}"

echo "==> Registering binfmt_misc handler '${NAME}'"
echo "    interpreter: ${INTERP}"
echo "    fingerprint: ${MAGIC} (mask ${MASKMASK})"

# binfmt_misc registration is BEST-EFFORT and non-fatal. GitHub Actions hosted
# runners frequently have no writable host binfmt_misc (`qemu-user-binfmt` may
# already hold the name -> EINVAL, or /proc/sys/fs/binfmt_misc is absent ->
# ENOENT). The gate's real execution paths do NOT depend on it: a crate-test
# binary and any IPC child it spawns stay inside the SAME QEMU user-mode
# process tree (QEMU re-execs itself for the child), and Cargo's `runner`
# already executes foreign binaries explicitly. Post-release download-and-run
# re-verification uses docker/setup-qemu-action, which registers binfmt
# reliably inside its container VM. So a registration failure only forfeits
# the "exec a foreign ELF straight from a bare host process" convenience, not
# any acceptance gate — warn and continue rather than abort.
_registered=0
if [[ -e /proc/sys/fs/binfmt_misc/register ]]; then
  # Drop any stale handler so a rerun is idempotent.
  if [[ -e "$REG" ]]; then
    echo -1 | sudo tee "$REG" >/dev/null 2>&1 || true
  fi
  # binfmt_misc registration line:
  #   :name:type:offset:magic:mask:interpreter:flags
  LINE=":${NAME}:M:0:${MAGIC}:${MASKMASK}:${INTERP}:PF"
  if printf '%b\n' "$LINE" | sudo tee /proc/sys/fs/binfmt_misc/register >/dev/null 2>&1 \
      && [[ -e "$REG" ]]; then
    _registered=1
  fi
fi

if [[ "$_registered" == "1" ]]; then
  echo "==> binfmt_misc '${NAME}' registered:"
  sudo cat "$REG"
else
  echo "==> WARN: binfmt_misc handler '${NAME}' not registered"
  echo "    /proc/sys/fs/binfmt_misc is not writable on this host"
  echo "    (or the name is already held). Continuing -- Cargo's QEMU runner"
  echo "    and in-tree IPC still execute foreign binaries without it."
  return 0 2>/dev/null || exit 0
fi