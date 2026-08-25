# RillML Platform Support Audit — 2026-08-26

## Scope and identity

- Repository: `hello-yunshu/rill-ml`
- Starting Stable baseline: `v1.5.2` / `3cbe4cc30cffeb4855e378a9d43e72cccc55ff1b`
- Existing PR: `#36`, initial head `e218096579e2073ca3e097ea2bb89e062233d013`
- Candidate release: `v1.5.3`
- Current phase: local pre-push validation for the existing PR branch

This audit records policy and local evidence separately from GitHub Actions,
release publication, and post-release download evidence. Those remote gates
remain `NOT_RUN` until this branch is pushed and the normal workflows complete.

## Implemented in this working tree

- Wasmtime 46 enables the upstream `pulley` feature alongside Cranelift.
- Runtime has additive `diagnostics` output for backend, pointer width,
  endianness, architecture, and OS.
- Core and default Full Runtime paths are separated in the musl gates.
- Signed model and signed WASM handler packs are created, inspected, loaded,
  and invoked by the musl smoke paths.
- Generic `Linux musl Stable` naming replaces the old OpenWrt feasibility
  workflow semantics.
- Five feasible Rust 1.94 musl targets are in the support policy and release
  matrix: x86_64, aarch64, riscv64, armv7, and i686.

## Explicitly skipped

`mips-unknown-linux-musl` and `mipsel-unknown-linux-musl` are skipped because
the pinned Rust 1.94 host toolchain has no distributable rustup standard
library components for those targets. They are not v1.5.3 Stable claims or
release assets. This is an execution boundary requested by the user, not a
claim that MIPS runtime qualification passed.

## Local evidence

| Check | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo check --locked -p rill-runtime --features wasm` | PASS |
| `cargo test --locked -p rill-runtime --features wasm --lib` | PASS (76 tests) |
| platform support unit tests | PASS (4 tests) |
| `scripts/platform_consistency.py --json` | PASS |
| `scripts/check_action_pins.py --json` | PASS |
| release version/compatibility checks | PASS |
| shell syntax checks | PASS |
| GitHub Actions for this working tree | NOT_RUN |
| v1.5.3 release and public asset re-download | NOT_RUN |

Local checks prove source consistency and host compilation only. They do not
prove target-architecture execution, remote CI, publication, or post-release
verification.
