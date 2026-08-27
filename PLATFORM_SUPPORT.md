# Platform Support

This file is the single user-facing statement of which platforms RillML
**officially supports**. A product surface is listed as **Supported** only when
it passes the corresponding acceptance gate — not merely because a Rust target
exists or `cargo build` succeeds.

The acceptance definition:

```text
reproducible build
+ real execution on the target architecture
+ full Core test suite
+ full Runtime test suite when Full Runtime is listed
+ signed models and handlers actually load
+ complete security boundary
+ release artifact re-verified
+ continuous CI
```

GitHub Actions is the primary acceptance environment. Everything is accepted
through Actions — no local Docker is required of users.

Verification preference (highest first):

native runner
→ direct QEMU
→ VM
→ Docker when it is the simplest reliable option.

There are exactly two platform states:

1. **Supported / Stable**
2. **Not listed** (platforms not yet accepted must not be claimed)

P0/P1/P2, Tier 1/2/3, Preview/Experimental, Alpha/Beta are never used as
user-visible platform status.

---

## Supported Platforms

Rows below have passed the Core gate. The Runtime column is an independent
surface: `✅` means Full Runtime, while `not listed` means that only Core is
qualified and the signed Stable index must not select the target as a Runtime.

| Target | Core | Runtime | Binding | Docker/QEMU | Execute | CI | Release |
|---|---|---|---|---:|---|---|---|
| `x86_64-unknown-linux-gnu` | ✅ | ✅ | Rust | ✅ | ✅ | ✅ | ✅ |
| `x86_64-unknown-linux-musl` | ✅ | ✅ | Rust | ✅ | ✅ | ✅ | ✅ |
| `aarch64-unknown-linux-gnu` | ✅ | ✅ | Rust | ✅* | ✅* | ✅ | ✅ |
| `aarch64-unknown-linux-musl` | ✅ | ✅ | Rust | ✅* | ✅* | ✅ | ✅ |
| `riscv64gc-unknown-linux-musl` | ✅ | ✅ | Rust | ✅ | ✅ | ✅ | ✅ |
| `armv7-unknown-linux-musleabihf` | ✅ | ✅ | Rust | ✅ | ✅ | ✅ | ✅ |
| `i686-unknown-linux-musl` | ✅ | ✅ | Rust | ✅ | ✅ | ✅ | ✅ |
| `x86_64-pc-windows-msvc` | ✅ | ✅ | Rust | – | ✅ | ✅ | ✅ |
| `aarch64-pc-windows-msvc` | ✅ | ✅ | Rust | – | ✅ | ✅ | ✅ |
| `aarch64-apple-darwin` | ✅ | ✅ (unsigned) | Rust | – | ✅ | ✅ | ✅ |
| `riscv64gc-unknown-linux-gnu` | ✅ | ✅ | Rust | ✅ | ✅ | ✅ | ✅ |
| `armv7-unknown-linux-gnueabihf` | ✅ | not listed | Rust | ✅* | ✅* | ✅ | investigation only |
| `s390x-unknown-linux-gnu` | ✅ | not listed | Rust | ✅* | ✅* | ✅ | investigation only |
| `powerpc64le-unknown-linux-gnu` | ✅ | not listed | Rust | ✅* | ✅* | ✅ | investigation only |
| `loongarch64-unknown-linux-gnu` | ✅ | ✅ | Rust | ✅ | ✅ | ✅ | ✅ |
| `x86_64-unknown-freebsd` | ✅ | ✅ | Rust | – | ✅ | ✅ | ✅ |

\* ARM64 GNU/musl Core and Runtime are executed under Docker + QEMU/binfmt on
x86_64 hosts, and natively when an ARM64 host is available. The same applies to
the niche Linux targets armv7, s390x, and powerpc64le, which are real-executed
under Docker + QEMU/binfmt for Core. The supported musl armv7 and i686 rows use
the Pulley32 backend and are separately executed under direct QEMU.

**RISC-V 64** (`riscv64gc-unknown-linux-gnu`) is cross-compiled and published as
part of the release asset matrix. Its source Stable Gate runs the full test
suite and the full default WASM Runtime under DIRECT user-mode QEMU on the
Actions host (`scripts/qemu-cross-gate.sh`), and its published release asset is
downloaded and re-executed under the same direct QEMU config
(`scripts/post-release-qemu-verify.sh`). It has no dependency on an upstream
`rust:*` container manifest.

**LoongArch64** (`loongarch64-unknown-linux-gnu`) is cross-compiled and
published as part of the release asset matrix. Its source Stable Gate uses a
pinned, checksum-verified LoongArch cross toolchain + `qemu-loongarch64` direct
user-mode (`scripts/loongarch-cross-gate.sh`), with full WASM handler support
through the Pulley backend. Its published release asset is downloaded and
re-executed under the same target environment
(`scripts/post-release-qemu-verify.sh`).

The **Release** column reflects the published asset matrix. `investigation only`
means the Core build may be retained as a separately named Actions artifact,
but no binary is placed in the formal GitHub Release or signed Stable
release-index Runtime selection. The Stable Runtime assets are:
`linux-x86_64` (GNU), `linux-x86_64-musl`, `linux-aarch64` (GNU),
`linux-aarch64-musl`, `linux-riscv64`, `linux-riscv64-musl`,
`linux-armv7-musl`, `linux-i686-musl`, `linux-loongarch64`,
`freebsd-x86_64`, `windows-x86_64`, `windows-aarch64`, and `macos-aarch64`
(see `scripts/build-release-index.py`).
The OHOS ARM64 binary is additionally published as the explicit release-only
asset `rill-runtime-<version>-ohos-aarch64`. It is intentionally not selected
by the Stable index: the official OHOS SDK and full default-feature Runtime
source qualification are available, but no HarmonyOS/OpenHarmony device is
available in the current environment for the real-execution acceptance gate.
The Linux assets are cross-compiled under Docker on the GitHub-Actions host;
the musl and ARM64 Linux assets are now published alongside the x86_64 GNU
asset.

### Notes

- **Full Runtime** means the full `rill-runtime` product: `--help`,
  `inspect-pack`, `inspect-handler`, `serve`, signed model/handler loading,
  WASM sandbox, and newline-delimited IPC.
- **Windows ARM64** (`aarch64-pc-windows-msvc`) is built, executed, and
  released on the native GitHub-hosted `windows-11-arm` runner. Docker cannot
  validate Windows native behaviour (no Windows container support on the Linux
  Docker hosts), so the native ARM64 runner is the accepted verification path.
  The full workspace suite, the Runtime smoke, and the signed-WASM Handler
  invoke all run on that runner.
- **macOS** remains Apple Silicon only. Intel macOS Runtime is intentionally
  not published; the Core library remains available on Intel macOS.
- **macOS unsigned policy:** the macOS aarch64 Runtime is always built and
  uploaded, but is unsigned and not notarized when Apple Developer ID secrets
  are absent (the permanent default). See `STABILITY.md` § macOS unsigned
  policy.
- **FreeBSD** (`x86_64-unknown-freebsd`) is built and released as a native
  FreeBSD binary. Docker + QEMU cannot run the FreeBSD kernel, so real
  execution is verified in a native FreeBSD VM (`vmactions/freebsd-vm` →
  `scripts/freebsd-ci.sh` on every push, and `post-release-verify-freebsd`
  re-verifies the published asset after each release).

- **Core-only Linux targets** (`armv7-unknown-linux-gnueabihf`,
  `s390x-unknown-linux-gnu`, and `powerpc64le-unknown-linux-gnu`) are not
  Full Runtime claims. Their Core investigation binaries are retained only as
  separately named Actions artifacts; they are not GitHub Release assets, are
  not selected by the signed Stable index, and must not be treated as evidence
  of signed WASM handler support.

---

## Support Contract

For every listed platform, **Supported** means all of the following are
continuously verified:

- **Build** — reproducible `cargo check` / `cargo build`.
- **Execute** — the target-architecture test binaries are really executed, not
  only cross-compiled.
- **CI** — a continuous job covers the platform (native runner → direct QEMU
  → VM → Docker when it is the simplest reliable option).
- **Runtime tests** — `rill-runtime` smoke, signed `.rillpack` / `.rillhandler`
  load, and IPC where the Runtime applies.
- **Release verification** — the released artifact is re-downloaded and
  executed, not just built.

---

## Bindings / Environments

Language and environment bindings are separate from OS/CPU platforms. A
binding is listed when it is built and exercised, independent of the OS/CPU
matrix.

| Binding | Status | Verification |
|---|---|---|
| Rust (native) | ✅ | full test suite on every supported platform |
| Python (`rill_ml`) | ✅ | `maturin` build + `pytest`; Linux x86_64 wheel |
| Browser JavaScript / WASM | ✅ | Docker-based WASM gate + publishable npm package: web ESM build (`pkg/`) + `wasm-pack test --node` |
| Node.js | ✅ | Docker-based WASM gate + publishable npm package: Node CJS build (`pkg-node/`) + `node tests/node-smoke.mjs` |
| Kotlin / Android | not listed | C FFI + JNI 构建链已就绪（Docker NDK 交叉构建 + CI）；真机/模拟器执行验证待补齐 |
| Swift / iOS | not listed | C FFI + Swift/XCFramework 构建链已就绪（macOS Xcode + CI）；真机/模拟器执行验证待补齐 |

Python and Node.js are bindings, not OS/CPU platforms, and are tracked
separately from the platform matrix.

---

## Not Listed (technical reasons)

These targets are not yet claimed as Supported. Reasons are recorded so the
distinction between "not yet accepted" and "unsupported upstream" is clear.

| Target | Intended role | Current blocker |
|---|---|---|
| `aarch64-linux-android` | Core FFI | FFI 构建链 + CI 已就绪（Docker NDK 交叉构建）；真机/模拟器执行验证未完成，未列入 Supported |
| `aarch64-apple-ios` | Core FFI | FFI 构建链 + CI 已就绪（macOS Xcode XCFramework）；真机/模拟器执行验证未完成，未列入 Supported |
| `aarch64-unknown-linux-ohos` | Release-only ARM64 Runtime | official OHOS SDK 交叉构建、Full Runtime source qualification 与 Pulley64 software regression 可用；正式 Release asset available；HarmonyOS/OpenHarmony 真机执行证据不可用，未列入 Supported |
| `mips-unknown-linux-musl` | skipped | Rust 1.94 rustup has no distributable target std artifact |
| `mipsel-unknown-linux-musl` | skipped | Rust 1.94 rustup has no distributable target std artifact |

A "not listed" target is **not** a promise. It is added only when it passes
the full acceptance gate.

The machine-readable support policy / claim registry is
[`platform-support.toml`](platform-support.toml). It records only targets that
have completed the relevant Core/Full Runtime
acceptance path; adding a row alone never grants support. The consistency gate
checks this registry against the platform table, CI source gates, release-index
patterns, and post-release verification.

---

## Verification Infrastructure

GitHub Actions is the enforcement environment. Verification preference is
native runner → direct QEMU → VM → Docker when it is the simplest reliable
option. The reproducible entry points are:

| Artifact | Purpose |
|---|---|
| `Dockerfile.test` | Pinned Rust image running the full native gate (fmt, clippy, test, doc) |
| `scripts/docker-test.sh` | Native Linux x86_64 GNU gate |
| `scripts/cross-exec-test.sh` | Real cross-architecture execution via Docker + QEMU/binfmt |
| `scripts/docker-release-smoke.sh` | Release build + signed-pack + IPC smoke in Docker |
| `scripts/docker-build-wheels.sh` | Python manylinux abi3 wheels for Linux x86_64/ARM64 |
| `scripts/docker-wasm-build.sh` | WASM bindings build + test (web ESM + Node CJS + wasm-bindgen suite) |
| `scripts/docker-build-android.sh` | Android FFI cross-build via Docker NDK (aarch64 + x86_64 staticlibs + JNI shim) |
| `scripts/build-ios-framework.sh` | iOS FFI XCFramework build (macOS Xcode, device + simulator) |
| `scripts/qemu-cross-gate.sh` | Direct QEMU user-mode gate (riscv64 / aarch64 / armv7 / s390x / powerpc64le): full Core suite and target-specific Runtime smoke, no Docker |
| `scripts/loongarch-cross-gate.sh` | LoongArch64 Stable gate: pinned cross toolchain + `qemu-loongarch64` user-mode, full WASM handler support through Pulley |
| `scripts/register_qemu_binfmt.sh` | Register QEMU binfmt_misc handlers for direct foreign ELF execution |
| `scripts/post-release-qemu-verify.sh` | Post-release re-verify of published Full Runtime RISC-V64 / LoongArch64 assets under direct QEMU |
| `scripts/freebsd-ci.sh` | FreeBSD native gate (fmt/clippy, full test suite, Full Runtime smoke) — executed in a FreeBSD VM via `vmactions/freebsd-vm` |
| `scripts/ohos-build.sh` | Official OHOS SDK cross-build of the default-feature ARM64 Runtime; static ELF identity check, no device claim |
| `scripts/ohos-release-verify.sh` | Post-release OHOS ARM64 ELF identity and non-GNU-loader check |
| `.github/workflows/cross-platform.yml` | CI: native / Docker gates, direct-QEMU cross gates, release smoke, Android/iOS FFI gates, FreeBSD VM native gate |

The Rust toolchain is pinned in `Dockerfile.test` (`RUST_PIN`, default matches
the verified toolchain and is >= the MSRV 1.94.0) for reproducibility.

The Linux `rill-runtime` release assets (`linux-x86_64-musl`, `linux-aarch64`,
`linux-aarch64-musl`) are cross-compiled under Docker on the GitHub-Actions
host in the release pipeline; the ARM64 assets are additionally smoke-tested
under Docker + QEMU/binfmt.

---

## Cross-Architecture State Compatibility

Persisted state must round-trip identically regardless of the CPU architecture
that wrote it. RillML enforces this as follows:

- **Wire format is JSON** (`serde_json`). The serialized form has no byte-order,
  pointer-width, or architecture-layout dependence: an `arm_count` or
  `feature_count` written on a 64-bit host is the same decimal JSON number a
  32-bit host reads. Every stable state type is persisted only through
  `serde_json` (see `Snapshot::from_json_validated`), never through a
  width-sensitive binary format.
- **Versioned portable DTOs** (`AdwinPortableStateV1`, `KswinPortableStateV1`,
  `PageHinkleyPortableStateV1`) are the stable persistence surface for the drift
  detectors. Their wire form is `serde_json` and the schema is versioned and
  `deny_unknown_fields`-locked, so the serialized shape is independent of the
  CPU that wrote the state. Most fields are already fixed-width (`u32`/`u64`/
  `i64`/`f64`); the frozen V1 DTOs carry a small number of in-memory `usize`
  fields (`AdwinPortableStateV1::max_window`,
  `KswinPortableStateV1::{window_size, check_interval}`) that serialize as plain
  decimal JSON numbers, so they are width-safe on the wire. They are frozen at
  the 1.0 baseline and must not be widened without a semver-major bump.
- **Legacy v1 stable types** (the 26 types with `v0.13.0` + `v1` fixtures) keep
  `usize`/`isize` in memory for counts and dimensions (e.g. `arm_count`,
  `feature_count`). This is intentional: those fields are private, they are
  serialized as plain decimal JSON numbers, and the `v1` golden fixtures prove
  byte-identical round-trips on every supported architecture. They are
  therefore platform-safe on the wire even though their in-memory type is
  architecture-width. **Any *new* stable state DTO must use fixed-width
  integers (`u32`/`u64`/`i32`/`i64`) only**, so no future DTO ships an
  in-memory width-sensitive field.
- **Golden fixtures** under `tests/fixtures/state/portable-v1/` and
  `tests/fixtures/state/v1/` are loaded and validated by the test suite on every
  supported architecture (native, Docker+QEMU, and direct QEMU), proving cross-arch stability
  by byte comparison, not by re-serializing in memory.
- **`serde(deny_unknown_fields)`** on portable DTOs rejects schema drift: a
  fixture or persisted state with unexpected fields fails closed rather than
  silently accepting a future-format blob.
