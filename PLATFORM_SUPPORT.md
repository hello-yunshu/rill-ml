# Platform Support

This file is the single user-facing statement of which platforms RillML
**officially supports**. A platform is listed as **Supported** only when it
passes the full acceptance gate — not merely because a Rust target exists or
`cargo build` succeeds.

The acceptance definition (see §51 of the cross-platform execution prompt):

```text
reproducible build
+ real execution on the target architecture
+ full Core test suite
+ full Runtime test suite (where applicable)
+ signed models and handlers actually load
+ complete security boundary
+ release artifact re-verified
+ continuous CI
```

Everything that can be Dockerized MUST be Dockerized first. Only where Docker
cannot provide valid verification do we fall back to native runners, VMs, or
dedicated hardware.

There are exactly two platform states:

1. **Supported / Stable**
2. **Not listed** (platforms not yet accepted must not be claimed)

P0/P1/P2, Tier 1/2/3, Preview/Experimental, Alpha/Beta are never used as
user-visible platform status.

---

## Supported Platforms

Only targets that pass the full gate are listed here.

| Target | Core | Runtime | Binding | Docker | Execute | CI | Release |
|---|---|---|---|---:|---|---|---|
| `x86_64-unknown-linux-gnu` | ✅ | ✅ | Rust | ✅ | ✅ | ✅ | ✅ |
| `x86_64-unknown-linux-musl` | ✅ | ✅ | Rust | ✅ | ✅ | ✅ | ✅ |
| `aarch64-unknown-linux-gnu` | ✅ | ✅ | Rust | ✅* | ✅* | ✅ | ✅ |
| `aarch64-unknown-linux-musl` | ✅ | ✅ | Rust | ✅* | ✅* | ✅ | ✅ |
| `x86_64-pc-windows-msvc` | ✅ | ✅ | Rust | – | ✅ | ✅ | ✅ |
| `aarch64-pc-windows-msvc` | ✅ | ✅ | Rust | – | ✅ | ✅ | ✅ |
| `aarch64-apple-darwin` | ✅ | ✅ (unsigned) | Rust | – | ✅ | ✅ | ✅ |

\* ARM64 GNU/musl Core and Runtime are executed under Docker + QEMU/binfmt on
x86_64 hosts, and natively when an ARM64 host is available.

The **Release** column reflects the published `rill-runtime` asset matrix:
`linux-x86_64` (GNU), `linux-x86_64-musl`, `linux-aarch64` (GNU),
`linux-aarch64-musl`, `windows-x86_64`, `windows-aarch64`, and `macos-aarch64`
(see `scripts/build-release-index.py`). The Linux assets are cross-compiled
under Docker on the GitHub-Actions host; the musl and ARM64 Linux assets are
now published alongside the x86_64 GNU asset.

### Notes

- **Runtime** means the full `rill-runtime` product: `--help`,
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

---

## Support Contract

For every listed platform, **Supported** means all of the following are
continuously verified:

- **Build** — reproducible `cargo check` / `cargo build`.
- **Execute** — the target-architecture test binaries are really executed, not
  only cross-compiled.
- **CI** — a continuous job covers the platform (Docker/QEMU wherever
  possible, native runner otherwise).
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
| Browser JavaScript / WASM | ✅ | Docker-first WASM gate + publishable npm package: web ESM build (`pkg/`) + `wasm-pack test --node` |
| Node.js | ✅ | Docker-first WASM gate + publishable npm package: Node CJS build (`pkg-node/`) + `node tests/node-smoke.mjs` |
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
| `riscv64gc-unknown-linux-gnu` | Core + Runtime | Execute path under evaluation (Docker/QEMU layer issue on some hosts); not yet accepted |
| `s390x-unknown-linux-gnu` | Core + Runtime | Execute path under evaluation; not yet accepted |
| `armv7-unknown-linux-gnueabihf` | Core (Runtime separately) | 32-bit Runtime acceptance pending; not yet accepted |
| `powerpc64le-unknown-linux-gnu` | Core + Runtime | Execute path under evaluation; not yet accepted |
| `loongarch64-unknown-linux-gnu` | Core + Runtime | Toolchain/execute availability; not yet accepted |
| `x86_64-unknown-freebsd` | Core + Runtime | 原生 VM CI 已接线（`vmactions/freebsd-vm` → `scripts/freebsd-ci.sh`）；首次真实执行验证待 CI 通过，未列入 Supported |
| `aarch64-linux-android` | Core FFI | FFI 构建链 + CI 已就绪（Docker NDK 交叉构建）；真机/模拟器执行验证未完成，未列入 Supported |
| `aarch64-apple-ios` | Core FFI | FFI 构建链 + CI 已就绪（macOS Xcode XCFramework）；真机/模拟器执行验证未完成，未列入 Supported |

A "not listed" target is **not** a promise. It is added only when it passes
the full acceptance gate.

---

## Verification Infrastructure

Docker-first is the enforcement principle. The reproducible entry points are:

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
| `scripts/freebsd-ci.sh` | FreeBSD native gate (fmt/clippy, full test suite, Runtime smoke) — executed in a FreeBSD VM via `vmactions/freebsd-vm` |
| `.github/workflows/cross-platform.yml` | CI: Docker native gate, QEMU cross-execution, release smoke, Android/iOS FFI gates, FreeBSD VM native gate |

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
  supported architecture (native and Docker+QEMU), proving cross-arch stability
  by byte comparison, not by re-serializing in memory.
- **`serde(deny_unknown_fields)`** on portable DTOs rejects schema drift: a
  fixture or persisted state with unexpected fields fails closed rather than
  silently accepting a future-format blob.