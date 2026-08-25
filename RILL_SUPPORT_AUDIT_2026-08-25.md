# RillML Platform Support Audit — 2026-08-25

## Scope and baseline

- Repository: `hello-yunshu/rill-ml`
- Stable group: `1.5.2`
- Preview group: `0.15.0`
- Compatibility authoritative baseline: `1.5.1`
- Historical baselines: `1.5.0`, `1.3.0`, `1.2.0`, `1.0.0`
- Audited baseline commit and `v1.5.2` tag: `3cbe4cc30cffeb4855e378a9d43e72cccc55ff1b`
- This audit branch: `codex/rill-1.5.2-openwrt-musl-support`

The tag and commit identity were verified in the local Git object database.
The branch has not yet been pushed, so exact-SHA GitHub Actions, a new Release,
and post-release download verification are explicitly `NOT_RUN` in this audit.
The pre-existing v1.5.2 public release is not changed by this branch.

## P0 findings and repairs

### Full Runtime feature drift

The direct-QEMU matrix used `--no-default-features` for
`armv7-unknown-linux-gnueabihf`, `s390x-unknown-linux-gnu`, and
`powerpc64le-unknown-linux-gnu`, while the platform table called all three
Runtime Supported. This is insufficient evidence for signed WASM Runtime
support.

Repair: these targets remain `Core Supported` and `Full Runtime: Not listed`.
Their investigation assets may still be built, but `build-release-index.py`
omits them from the signed Stable Runtime selection. The release asset verifier
allows those explicitly registered `asset only` files, and post-release Full
Runtime verification no longer treats them as Stable Runtime artifacts.

### Documentation and workflow drift

The contradictory armv7 and FreeBSD comments were corrected. README Chinese,
README English, STABILITY, RUNTIME, PLATFORM_SUPPORT, CHANGELOG, workflow
comments, and the OpenWrt consumer document now describe Core and Full Runtime
as separate surfaces.

### SemVer baseline drift

`pipeline.yml` no longer hard-codes `1.5.0` as the admission baseline.
`scripts/release_compatibility.py` reads the authoritative and historical lists
from `release-plan.toml`; CI uses one authoritative baseline for all Stable
crates and loops over historical baselines as sanity gates.

### Machine-readable support source

`platform-support.toml` records only evidence-qualified surfaces and includes
target OS/architecture/libc, feature set, source gate, release suffix, and
post-release gate. `scripts/platform_consistency.py` fails closed when docs,
source gates, release-index patterns, post-release paths, or feature semantics
drift. The gate is wired into the release-index helper job.

## OpenWrt / musl expansion

`scripts/musl-cross-gate.sh` and the manual `OpenWrt musl feasibility` workflow
provide a fail-closed QEMU/Zig path for:

- `riscv64gc-unknown-linux-musl`;
- `armv7-unknown-linux-musleabihf`;
- `i686-unknown-linux-musl`;
- `mips-unknown-linux-musl`;
- `mipsel-unknown-linux-musl`.

The gate covers target compile, Core tests, docs, default-WASM Runtime build,
signed model pack verification, inspect, IPC handshake, and resource RSS
reporting. It does not itself grant support, publish an asset, or create a
Consumer qualification claim. These targets remain `Not listed` until the
manual workflow, release asset, and post-release evidence exist.

## Evidence run on this branch

- `python3 -m unittest discover -s scripts/tests -v`: **PASS, 131 tests**.
- `python3 scripts/platform_consistency.py --json`: **PASS**.
- `python3 scripts/check_action_pins.py --json`: **PASS**.
- `python3 scripts/check_product_surface.py --json`: **PASS**.
- `git diff --check`: **PASS**.
- `cargo fmt --all --check`: **PASS**.
- `cargo test --locked -p rill-runtime-protocol --lib --tests`: **PASS**
  (15 unit tests, 38 IPC tests, 4 Preview fixture tests).
- `cargo test --locked -p rill-runtime --lib --tests`: **PASS**
  (76 library tests, 2 binary tests, 6 process tests, 5 stateful tests,
  17 WASM tests, 6 WIT component tests).
- `cargo metadata --format-version 1 --locked`: **PASS** after the required
  network access to the configured Cargo mirror; the initial sandbox attempt
  failed only on DNS resolution.
- `OpenWrt musl feasibility` Actions workflow: **NOT_RUN**.
- Exact-SHA CI / Cross-Platform / Security workflows for this branch:
  **NOT_RUN** (branch not pushed at audit time).
- New Release asset download and post-release requalification:
  **NOT_RUN**.

## Target matrix

| Target | Core | Full Runtime | libc | Backend | CI real execute | Release | Post-release verify | Consumer-qualified |
|---|---|---|---|---|---|---|---|---|
| `x86_64-unknown-linux-gnu` | Supported | Supported | gnu | Cranelift | baseline + Actions required | indexed | container gate | Not listed |
| `x86_64-unknown-linux-musl` | Supported | Supported | musl | Cranelift | baseline + Actions required | indexed | container gate | Not listed |
| `aarch64-unknown-linux-gnu` | Supported | Supported | gnu | Cranelift | baseline + Actions required | indexed | container gate | Not listed |
| `aarch64-unknown-linux-musl` | Supported | Supported | musl | Cranelift | baseline + Actions required | indexed | container gate | Not listed |
| `riscv64gc-unknown-linux-gnu` | Supported | Supported | gnu | Pulley/target gate | baseline + Actions required | indexed | direct QEMU | Not listed |
| `loongarch64-unknown-linux-gnu` | Supported | Supported | gnu | Pulley | baseline + Actions required | indexed | direct QEMU | Not listed |
| `x86_64-pc-windows-msvc` | Supported | Supported | msvc | Cranelift | baseline + Actions required | indexed | native | Not listed |
| `aarch64-pc-windows-msvc` | Supported | Supported | msvc | Cranelift | baseline + Actions required | indexed | native | Not listed |
| `aarch64-apple-darwin` | Supported | Supported | none | Cranelift | baseline + Actions required | indexed | native | Not listed |
| `x86_64-unknown-freebsd` | Supported | Supported | none | Cranelift | baseline + Actions required | indexed | FreeBSD VM | Not listed |
| `armv7-unknown-linux-gnueabihf` | Supported | Not listed | gnu | target-specific | Actions required | asset only | not applicable | Not listed |
| `s390x-unknown-linux-gnu` | Supported | Not listed | gnu | target-specific | Actions required | asset only | not applicable | Not listed |
| `powerpc64le-unknown-linux-gnu` | Supported | Not listed | gnu | target-specific | Actions required | asset only | not applicable | Not listed |
| `riscv64gc-unknown-linux-musl` | Not listed | Not listed | musl | target-dependent | manual Actions not run | no asset | not run | Not listed |
| `armv7-unknown-linux-musleabihf` | Not listed | Not listed | musl | Pulley32 candidate | manual Actions not run | no asset | not run | Not listed |
| `i686-unknown-linux-musl` | Not listed | Not listed | musl | Pulley32 candidate | manual Actions not run | no asset | not run | Not listed |
| `mips-unknown-linux-musl` | Not listed | Not listed | musl | target-dependent | manual Actions not run | no asset | not run | Not listed |
| `mipsel-unknown-linux-musl` | Not listed | Not listed | musl | target-dependent | manual Actions not run | no asset | not run | Not listed |

“Supported” in this table is a contract status inherited from the v1.5.2
baseline and still requires a new exact-SHA Actions run for this branch. No
new target is claimed Stable by this change.

## Consumer boundary

RillML retains only generic Core, Runtime, protocol, signed model/handler,
platform asset, and qualification surfaces. No `rill-pm-adapter`, OpenWrt UCI
writer, firewall mutator, or product-specific adapter was added back to the
workspace or release ownership. OpenWrt observation/action glue, mutation,
transaction, rollback, health checks, and reward/outcome remain Consumer-owned.
