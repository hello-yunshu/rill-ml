# Contributing to RillML

Thank you for your interest in RillML. This document describes how to
contribute effectively while preserving the project's core principles.

RillML is an online (single-pass, bounded-memory) machine learning library for
Rust. The bar for additions is **reliability and verifiability**, not algorithm
count. A small set of well-tested modules is preferred over a large set of
loosely verified ones.

## 1. Before you start

Please read:

- `README.md` (or `README.en.md`) — project scope, status, and disclaimers.
- `RillML_Roadmap(1).md` — long-term direction and per-version goals.
- `CHANGELOG.md` — what has already shipped.

If your contribution is substantial (new module, API change, or new
dependency), please open an issue first to discuss the design. Small fixes
(docs, typos, test improvements) can go straight to a pull request.

## 2. Project principles (non-negotiable)

Every contribution must respect:

1. **Online, single-pass, bounded memory.** No storing the full history. `O(1)`
   for non-rolling statistics, `O(d)` for linear models, `O(window)` for
   rolling statistics.
2. **`predict` is side-effect free.** State updates happen only in `learn` or
   `update`. Progressive evaluation follows `predict → metric.update → learn`.
3. **No panics in public APIs.** Return `Result<_, RillError>`. Validate inputs
   at the boundary.
4. **`f64` only.** Dense `&[f64]` slices. No `HashMap<String, f64>`.
5. **Concrete types for optimizers/losses.** No trait objects for these. Use
   enums so state remains serializable.
6. **Transformers never see the target `y`.** No label leakage in the
   progressive-evaluation sense.
7. **No domain-specific types.** RillML must not contain `Battery`, `Mouse`,
   `ChargingState`, `PollingRate`, `RGB`, HID, Tauri, or plugin-protocol
   types. These belong in the application layer.
8. **Optional `serde` only.** The default build must not require `serde`.
9. **No `no_std` claim.** Do not add a fake `std` feature.
10. **No CLI named `rill`.** Future inspect tools, if any, will be named
    `rillml-inspect` or similar.

## 3. Development setup

Requirements:

- Rust 1.85.0 or newer (MSRV is pinned to 1.85).
- `cargo`, `rustfmt`, `clippy` from the stable toolchain.

Common commands:

```bash
cargo fmt --check
cargo check
cargo check --features serde
cargo test
cargo test --features serde
cargo clippy --all-targets --features serde -- -D warnings
cargo doc --features serde --no-deps
RUSTDOCFLAGS="-D warnings" cargo doc --features serde --no-deps
cargo package
cargo package --features serde
```

Before pushing, please run the full release checklist locally (see
`.github/workflows/ci.yml`). CI will run the same set on Linux, macOS, and
Windows.

## 4. Adding a new module

A new module (statistic, model, metric, transformer, etc.) must come with:

- A clear mathematical definition in the module-level rustdoc, including the
  update rule and any numerical-stability considerations.
- Time and space complexity stated in the rustdoc.
- Boundary conditions: what happens with zero samples, one sample, non-finite
  inputs, dimension mismatches.
- Unit tests covering the happy path and the boundaries.
- A random comparison test against a reliable reference (offline batch
  formula, or a well-known implementation). Use `proptest` or a fixed-seed
  `ChaCha8Rng`.
- Serialization round-trip test under `tests/serialization.rs` when the type
  holds state.
- At least one example in the rustdoc or under `examples/`.

If any of these is missing, the PR will be held until it is addressed.

## 5. API stability and breaking changes

RillML is currently at `0.x`. Breaking changes are allowed but must be:

- Documented in `CHANGELOG.md` under `[Unreleased]` with a clear "Breaking"
  section.
- Avoided when an additive change would suffice.
- Discussed in an issue first if the change touches `OnlineRegressor`,
  `OnlineBinaryClassifier`, `Transformer`, `Metric`, `OnlineStatistic`, or
  `Snapshot<T>`.

`Snapshot<T>` carries a `format_version`. Bumping the format version requires a
migration note in `CHANGELOG.md`.

## 6. Code style

- Run `cargo fmt` before committing. `rustfmt.toml` is checked in.
- `cargo clippy --all-targets --features serde -- -D warnings` must pass.
- Public items have rustdoc with `///`. Include `# Errors` and `# Panics`
  sections where applicable (public APIs should not panic).
- Avoid unexplained abbreviations.
- No `println!`/`eprintln!` in library code. Examples and benches may print.
- No global mutable state. No implicit threads. No async inside the library.
- Floating-point comparisons in tests use tolerances (`approx` or explicit
  `abs()` checks), never `==` on computed values.
- Random examples and tests use fixed seeds so output is reproducible.

## 7. Tests

- Unit tests live in `#[cfg(test)] mod tests` inside each source file.
- Integration tests live under `tests/` and exercise cross-module behavior
  (pipeline, progressive order, serialization round-trip, learning
  convergence).
- Property-based tests use `proptest` with fixed seeds.
- Reference tests compare online results against batch formulas.
- Do not introduce tests that depend on wall-clock time, network access, or
  the filesystem.

## 8. Commit messages and pull requests

- Use the imperative mood: "Add RollingMSE", not "Added RollingMSE".
- Reference issues in the PR description, not in every commit.
- Keep PRs focused. A PR that mixes a new module with a refactor of unrelated
  code will be split.
- Fill in the pull request template (`.github/pull_request_template.md`).
- Make sure CI is green before requesting review.

## 9. Licensing

By contributing, you agree that your contributions will be licensed under
the MIT license, the same as the rest of RillML. Do not include
code that is incompatible with this license.

If you adapt code from another project, attribute it in
`THIRD_PARTY_NOTICES.md` and ensure the source license permits inclusion under
the MIT license.

## 10. What we do not want

Please do not open PRs for:

- A "dynamic, arbitrarily-composed pipeline" with trait objects and runtime
  composition. This is explicitly out of scope.
- `no_std` support. It is a future possibility, not a current goal.
- Python bindings, WASM bindings, or PyO3 wrappers in this crate. They belong
  in separate packages once the Rust core is stable.
- Domain-specific features (battery, mouse, HID, Tauri, plugin protocols).
- Copy-paste ports of River modules without independent verification.
- A CLI named `rill`.

These will be politely closed.

## 11. Getting help

Open a GitHub issue with the `question` label, or start a discussion in the
relevant issue thread. Be specific: include the Rust version, the RillML
version, a minimal reproducible example, and the expected vs. actual behavior.
