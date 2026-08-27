# Generic Linux/musl Support

RillML treats Linux/musl as a generic release surface. OpenWrt is a consumer
of that surface, not a RillML platform name or a Rill Runtime ownership
boundary.

## Stable policy

The current 1.5.x policy, introduced in v1.5.3, covers these feasible Rust
1.94 targets:

| Target | Backend | Width | Endianness | Source gate | Post-release gate |
|---|---|---:|---|---|---|
| `x86_64-unknown-linux-musl` | Cranelift | 64 | little | native musl | container |
| `aarch64-unknown-linux-musl` | Cranelift | 64 | little | native musl | container |
| `riscv64gc-unknown-linux-musl` | Cranelift | 64 | little | direct QEMU | direct QEMU |
| `armv7-unknown-linux-musleabihf` | Pulley32 | 32 | little | direct QEMU | direct QEMU |
| `i686-unknown-linux-musl` | Pulley32 | 32 | little | direct QEMU | direct QEMU |

The machine-readable claim source is
[`platform-support.toml`](../platform-support.toml). `Linux musl Stable` and
the release pipeline provide the source and post-release paths; they do not
turn a local build into execution evidence.

## Explicitly skipped targets

`mips-unknown-linux-musl` and `mipsel-unknown-linux-musl` are skipped for this
run because Rust 1.94 has no distributable rustup standard-library artifacts
for them on the pinned host toolchain. They are not current 1.5.x Stable claims
or release assets.

## Evidence boundary

Each target needs Core and default Full Runtime execution, signed model and
handler load/invoke, IPC smoke, release-index identity, and re-download
verification. Direct QEMU is target-architecture execution evidence, but it is
not native hardware evidence. OpenWrt package, rootfs, UCI, and adapter
qualification belongs to the consumer repository.
