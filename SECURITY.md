# Security Policy

## Supported versions

RillML is currently at `0.x` ("Experimental but usable"). Only the latest
minor release receives security fixes. There is no separate LTS branch.

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

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

- Always check `schema_version` before restoring state. A mismatch means the
  state format is not what the current code expects.
- `Snapshot<T>` carries `created_at` as a string for portability; do not
  interpret it as an authoritative timestamp for security decisions.
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

## Dependency policy

- Dev-dependencies (`approx`, `proptest`, `rand`, `rand_chacha`,
  `serde_json`, `criterion`) are not linked into downstream releases of
  `rill-ml` and do not affect the runtime attack surface of users who depend
  on this crate.
- Runtime dependencies (`thiserror`, optional `serde`) are kept to a minimum.
  Dependabot will open PRs for patched versions; maintainers will review and
  merge them promptly.
- RillML will never silently add a runtime dependency that executes code at
  build time beyond standard proc-macro crates.

## Changes to this policy

This document may be updated as RillML evolves. Material changes will be
noted in `CHANGELOG.md`.
