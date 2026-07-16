# Security Policy

## Supported versions

RillML is currently at `0.x` ("Experimental but usable"). Only the latest
minor release receives security fixes. There is no separate LTS branch.

| Version | Supported          |
| ------- | ------------------ |
| 0.8.x   | :white_check_mark: |
| < 0.8   | :x:                |

Once RillML reaches `1.0`, a more detailed support table will be published
here.

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

This policy covers the `rill-ml` crate published from this repository. It does
**not** cover:

- Issues in downstream applications that use RillML (report those to the
  application maintainer).
- Vulnerabilities in dependencies that are best reported upstream (e.g.,
  `serde`, `rand`). We will, however, bump affected dependency versions
  promptly once a fixed release is available.
- Theoretical numeric drift or model accuracy regressions. These are bugs, not
  security issues, and should be filed as regular GitHub issues.

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

### 6. Untrusted WASM handlers (rill-runtime 0.7+)

`rill-runtime` with the `wasm` feature loads signed `.rillhandler` WASM
components. The sandbox is designed to minimise trust in handler code:

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

- The `wasm` feature is opt-in. Default builds of `rill-runtime` do not
  include Wasmtime and cannot load `.rillhandler` packs.
- Wasmtime bug or misconfiguration could theoretically compromise the
  sandbox. Keep Wasmtime updated and monitor upstream security advisories.
- The fuel and epoch limits are best-effort; a handler that finds a
  sandbox escape in Wasmtime could bypass them. This is a trusted-platform
  assumption, not a guarantee against zero-day exploits.
- Handlers are loaded at startup and run for the lifetime of the process.
  Hot replacement is not supported in 0.7.

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
