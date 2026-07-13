## Summary

<!-- One or two sentences describing what this PR changes and why. -->

## Motivation

<!-- Which problem does this solve? Reference an issue if applicable:
`Fixes #123`, `Refs #123`. -->

## Scope check

- [ ] This change is online, single-pass, and bounded-memory.
- [ ] `predict` remains side-effect free (state updates only in `learn` /
      `update`).
- [ ] No panics in public APIs (returns `Result<_, RillError>`).
- [ ] Only `f64` and dense `&[f64]` slices; no `HashMap<String, f64>`.
- [ ] No domain-specific types (`Battery`, `Mouse`, `ChargingState`,
      `PollingRate`, `RGB`, HID, Tauri, plugin protocols).
- [ ] No new trait objects for optimizers/losses (concrete enums only).
- [ ] Optional `serde` only; default build does not require `serde`.
- [ ] No `no_std` claim and no CLI named `rill`.

## Changes

<!-- Bullet list of the actual changes. -->

-

## Tests

- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy --all-targets --features serde -- -D warnings` passes.
- [ ] `cargo test` passes (without `serde`).
- [ ] `cargo test --features serde` passes.
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --features serde --no-deps`
      passes.
- [ ] New module has unit tests covering boundary conditions.
- [ ] New module has a reference/property test against a batch formula or
      fixed-seed `ChaCha8Rng`.
- [ ] If the type holds state, `tests/serialization.rs` has a round-trip
      test.

## Documentation

- [ ] Public items have rustdoc.
- [ ] Complexity (time and space) is stated in rustdoc.
- [ ] `CHANGELOG.md` updated under `[Unreleased]` (with a `Breaking`
      section if applicable).
- [ ] `THIRD_PARTY_NOTICES.md` updated if a new dependency or adapted
      algorithm is introduced.
- [ ] Examples under `examples/` updated if the public API changed.

## Licensing

By submitting this pull request, I confirm that my contributions are
licensed under the MIT license, the same as RillML.
