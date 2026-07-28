# Security Policy

## Supported versions

RillML is on the stable 1.x release line. The Stable crates
(`rill-ml`, `rill-runtime`, `rill-runtime-protocol`, `rill-handler-api`)
are under the 1.x compatibility freeze documented in
[`STABILITY.md`](STABILITY.md). The Preview crates
(`rill-ml-python`, `rill-ml-wasm`, `rill-ml-tokio`, `rill-ml-arrow`,
`rill-ml-polars`, `rillml-inspect`) remain at `0.x` and are not covered by
the 1.x freeze.

| Version line | Status | Supported | Channel |
| ------------ | ------ | --------- | ------- |
| `1.0.x` | Current stable | :white_check_mark: | `local-ai-stable` |
| `1.0.0-rc.x` | Superseded candidate | :x: | `local-ai-candidate` |
| `0.13.x` | Previous stable line | :white_check_mark: (security fixes only) | Immutable releases |
| `< 0.13.0` | EOL | :x: | — |

The `local-ai-stable` pointer tracks the latest independently smoked final
release. The `local-ai-candidate` pointer preserves the latest RC. A candidate
release never updates the stable pointer.

### Backport policy

- Security fixes that affect a Stable crate are backported to the
  `0.13.0` release line as long as it appears in the supported versions
  table above.
- Security fixes that only affect a Preview crate are applied
  forward-only; Preview crates do not carry a backport guarantee.
- A fix lands on `main` for the next `1.0.x` release. If the supported
  `0.13.x` line is affected, the same fix is cherry-picked to that release
  branch.

## Reporting a vulnerability

Please **do not** open a public GitHub issue for a security vulnerability.

Instead, report it privately:

1. Open a GitHub Security Advisory using the **"Report a vulnerability"** button
   on the **Security** tab of the repository, **or**
2. Contact a maintainer directly via a private message on GitHub.

Please include:

- A description of the issue and its potential impact.
- A minimal reproduction (code snippet, input data, or `Cargo.toml` snippet).
- Affected RillML version and Rust toolchain version.
- Any suggested mitigation or fix.

We will acknowledge receipt within 7 days and aim to provide an initial
assessment within 30 days. Coordinated disclosure timelines will be agreed on
a per-case basis. Reporters will be credited in the release notes and in
`CHANGELOG.md` unless they prefer to remain anonymous.

## Scope

This policy covers the Stable crates published from this repository:

- `rill-ml` — the core learning library.
- `rill-runtime` — the standalone inference runtime (including the
  Wasmtime-sandboxed WASM handler loader when built with the `wasm`
  feature, which is the default).
- `rill-runtime-protocol` — the IPC v1/v2 JSON wire schema and stable
  error codes.
- `rill-handler-api` — the WIT ABI v1 contract for handler authors.

The Preview crates (`rill-ml-python`, `rill-ml-wasm`, `rill-ml-tokio`,
`rill-ml-arrow`, `rill-ml-polars`, `rillml-inspect`) ship from this
repository but are not under the 1.x compatibility freeze. Security
issues in Preview crates are still welcome and will be fixed, but they
do not carry the Stable backport guarantee.

This policy does **not** cover:

- Issues in downstream applications that use RillML (report those to the
  application maintainer).
- Vulnerabilities in dependencies that are best reported upstream (e.g.,
  `serde`, `rand`). We will, however, bump affected dependency versions
  promptly once a fixed release is available.
- Theoretical numeric drift or model accuracy regressions. These are bugs, not
  security issues, and should be filed as regular GitHub issues.

### macOS signing policy

Official `rill-runtime` macOS release assets are Apple Silicon (ARM64) only
and are published even when Apple Developer ID credentials are absent.
The permanent default is an unsigned, non-notarized binary accompanied by
`*.unsigned.json` status metadata; missing signing credentials never block
a candidate or stable release. Model packs, handler packs, and release
indexes remain Ed25519-signed and hash-verified independently of Apple code
signing. See [`STABILITY.md`](STABILITY.md) §Platform support for the full
platform matrix and first-launch authorization guidance.

### Wasmtime security upgrade policy

`rill-runtime` with the `wasm` feature (the default) depends on
`wasmtime`. A Wasmtime security advisory triggers an out-of-band release
of the affected Stable release lines (`1.0.x`; `0.13.x` only if the `wasm`
feature is in scope for that line).
Patch upgrades within the same wasmtime minor line are applied via
Dependabot PRs and reviewed by maintainers. Minor upgrades require a
maintainer review of the changelog before merge; the review note is
recorded in `CHANGELOG.md`.

## Threat model and safe usage

RillML is a library, not a service. It does not open sockets, spawn threads,
access the filesystem, or execute arbitrary code. The realistic attack
surface is therefore narrow, but the following points are worth noting:

### 1. Deserialization of untrusted `Snapshot<T>`

The optional `serde` feature allows serializing and deserializing
`Snapshot<T>`. **Do not deserialize `Snapshot<T>` from untrusted sources
without validation.**

- Always restore through `Snapshot::into_model()` (or
  `Snapshot::into_model_with_validation()`) so `format_version` is checked. A
  mismatch means the state format is not what the current code expects.
- The envelope validates only its own format version. It cannot infer the
  invariants of an arbitrary `T`. At a trust boundary, use
  `into_model_with_validation()` and reject state that violates your model or
  application limits before making it active.
- Bandit types, `StandardScaler`, `SparseFeatures`, and `FeatureHasher` also
  validate their internal invariants during deserialization. Do not assume all
  arbitrary serde-enabled types provide the same guarantee.
- Deserializing a `Snapshot<T>` with absurdly large arrays can allocate large
  amounts of memory. If you accept snapshots from untrusted input, cap the
  size of the incoming payload **before** handing it to `serde_json` or
  another deserializer.
- RillML does not implement custom `Drop` logic that could be abused through
  partially-initialized state, but you should still treat a deserialized
  model as untrusted until you have run it through your own validation.

### 2. Non-finite inputs

Public APIs return `RillError` when given `NaN`, `+inf`, or `-inf` inputs.
They do not propagate non-finite values silently. Do not bypass these checks
by constructing internal state directly.

### 3. Numerical stability

RillML uses Welford's algorithm for variance, a numerically stable sigmoid,
and clamped AdaGrad accumulators. Even so, extreme inputs (very large feature
magnitudes, very long runs, adversarial feature scales) can produce
non-finite internal state. If `predict` returns a non-finite value, treat the
model as unhealthy and reset or fall back to a baseline.

### 4. Randomness

All random examples and tests use fixed seeds (`ChaCha8Rng::seed_from_u64`)
so output is reproducible. RillML does **not** expose a cryptographic random
source and must not be used for cryptographic key generation, nonce
generation, or any security-critical randomness.

### 5. No sandboxing

RillML runs in the host process. It is not sandboxed. If you execute untrusted
example code or load untrained models from third parties, do so in a sandbox
appropriate to your platform.

### 6. Untrusted WASM handlers (rill-runtime 1.0+)

`rill-runtime` with the `wasm` feature (the default) loads signed
`.rillhandler` WASM components. The sandbox is designed to minimise trust
in handler code:

- **Signature before instantiation.** The handler pack's manifest, checksums,
  and Ed25519 signature are verified before any WASM byte is compiled. An
  unsigned or tampered pack is rejected at load time.
- **Trust domain separation.** Handler trust keys are independent of model
  trust keys. A model publisher key cannot authorise a handler, and vice
  versa.
- **Capability intersection.** Effective capabilities are the intersection of
  model manifest and handler manifest capabilities. The handler cannot
  advertise capabilities the model did not declare. The runtime rejects any
  `Invoke` request whose capability is not in the effective set.
- **Metadata consistency.** The guest's `metadata()` return value must match
  the signed manifest exactly (id, version, API version, capabilities). A
  mismatch causes load failure.
- **Wasmtime sandbox.** Handlers execute inside a Wasmtime component sandbox
  with: no WASI imports (no filesystem, network, environment, stdio, or
  process access), per-call fuel budget, epoch interruption for wall-clock
  timeout, memory growth capped at 64 MiB, table growth capped at 10 000
  elements, WASM stack capped at 1 MiB, and I/O JSON capped at 1 MiB.
- **Error containment.** Handler traps, timeouts, and invalid outputs are
  mapped to stable IPC error codes (`handlerTrap`, `handlerTimeout`,
  `handlerOutputTooLarge`, `handlerInvalidOutput`). The runtime process
  remains healthy after a handler failure and can continue serving requests.
- **No host path leakage.** Error messages returned to the IPC client do not
  include host filesystem paths, backtraces, or internal runtime state.

**Limitations to be aware of:**

- The `wasm` feature is on by default. Builds that pass
  `--no-default-features` do not include Wasmtime and cannot load
  `.rillhandler` packs; this must be documented by the downstream
  distributor.
- Wasmtime bug or misconfiguration could theoretically compromise the
  sandbox. Keep Wasmtime updated and monitor upstream security advisories.
- The fuel and epoch limits are best-effort; a handler that finds a
  sandbox escape in Wasmtime could bypass them. This is a trusted-platform
  assumption, not a guarantee against zero-day exploits.
- Handlers are loaded at startup and run for the lifetime of the process.
  Hot replacement is not supported in 1.x.

## Dependency policy

- Dev-dependencies (`approx`, `proptest`, `rand`, `rand_chacha`,
  `serde_json`, `criterion`) are not linked into downstream releases of
  `rill-ml` and do not affect the runtime attack surface of users who depend
  on this crate.
- Runtime dependencies (`thiserror`, `rand`, and optional `serde`) are kept to
  a minimum.
  Dependabot will open PRs for patched versions; maintainers will review and
  merge them promptly.
- RillML will never silently add a runtime dependency that executes code at
  build time beyond standard proc-macro crates.

## Changes to this policy

This document may be updated as RillML evolves. Material changes will be
noted in `CHANGELOG.md`.
