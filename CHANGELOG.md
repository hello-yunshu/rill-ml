# Changelog

All notable changes to RillML will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
with the Rust-specific convention that 0.x releases may break the public API.

> **Status: 1.x Stable.**
> The Stable crates (`rill-ml`, `rill-runtime`, `rill-runtime-protocol`,
> `rill-handler-api`) are released under the 1.x compatibility freeze. Preview
> crates (`rill-ml-python`, `rill-ml-wasm`, `rill-ml-tokio`, `rill-ml-arrow`,
> `rill-ml-polars`, `rillml-inspect`) remain at `0.x`. Final releases update
> `local-ai-stable` only after their immutable public assets pass the
> independent host smoke; `local-ai-candidate` remains the RC channel. Do not
> use RillML for
> safety-critical, medical, financial, or industrial-control decisions without
> independent verification. Always keep a simple baseline and business-rule
> fallback alongside model predictions.

## [Unreleased]





## [1.3.0] - 2026-08-22

### Changed

- Stable crates advance additively to `1.3.0`; Preview crates remain at
  `0.15.0` and are not published by the Stable release path.
- IPC v3 is explicitly documented and gated as a library-only Preview surface;
  the production Runtime CLI remains Stable IPC v1/v2 and fails closed when no
  explicit handler is selected.
- Added a fail-closed product-surface gate, consumer-owned adoption schema and
  validator, and deterministic offline/released conformance entrypoint.
- Added TrustMetadata v1 current/next key overlap, validity, revocation,
  emergency revoke, metadata-generation/digest rollback protection, and
  monotonic release-generation verification while preserving the frozen
  release-index v3 payload.
- Added real libFuzzer targets and seed corpora for archive, handler, index,
  trust metadata, IPC, snapshot, and handler JSON boundaries.
- Release assets now carry deterministic dependency-complete CycloneDX and SPDX
  SBOMs bound to the version, tag, commit, dependency graph, artifact names,
  sizes, and SHA-256 values; standard validators and the release-bound verifier
  run again after publication.
- Added FTRL feature-capacity diagnostics, DecisionLedger contract
  documentation, and an explicit C FFI unsafe trust-domain document.

## [1.2.0] - 2026-08-18

### Changed

- The Stable crates advance additively to `1.2.0`; Preview workspace crates
  stay at `0.15.0`. No 1.1 API, state, IPC, WIT, or release-index contract is
  removed or rewritten.
- The release-index schema advances to v3 and becomes an explicitly versioned
  Stable protocol: published schema versions stay immutable, later 1.x minors
  may add new schema versions, and older readers fail closed on unknown
  schemas. Linux Runtime/Adapter artifacts record `targetLibc`
  (`gnu`/`musl`) so GNU and musl builds of the same Linux OS+arch are
  disambiguated deterministically.
- A pure-Python `release-admission` gate admits a release only when its tag
  commit has successful same-SHA `CI / Release`, `Security audit`, and (when
  cross-platform-relevant paths changed) `Cross-Platform Verification` runs.
  Successful release assets stay immutable.
- Published-artifact verification now covers every Supported platform:
  RISC-V64/LoongArch64 under direct user-mode QEMU, FreeBSD on a native VM,
  and a `post-release-verify-native` matrix on native Windows x86_64 and
  Windows ARM64 runners, all re-verifying the actual GitHub Release assets
  via the existing `host_smoke.py`.
- `move-index-pointer` waits on the PM adapter host smoke and the native
  post-release verify before promoting the candidate/stable pointer, so a
  failed adapter or native verification can never be promoted.

### Added — OpenWrt Performance Manager decision adapter

- New Preview crate `rill-pm-adapter`: a Unix-domain-socket decision host
  speaking the independent `pm-rill-shadow` v1 contract (`status` /
  `observe` / `outcome`), with context- and goal-partitioned models, a
  bounded decision ledger with strict validated-outcome rejection rules,
  bounded persistent state with restart recovery, and model health
  reporting. The adapter is advisory-only and fail-closed on oversized
  frames.
- CI cross-builds the adapter for the musl Linux targets PM consumes and
  publishes it as a distinct `pm-adapter` release-index artifact kind with
  `pmAdapterProtocolVersion` 1; an independent released-adapter host smoke
  verifies the published binary against the signed index and exercises a
  real round-trip plus fail-closed negative cases.

## [1.2.0-rc.3] - 2026-08-18

### Changed

- A pure-Python `release-admission` gate now runs at the root of the Release
  tree on every `workflow_dispatch`, and only admits a release whose tag commit
  has a successful same-SHA `CI / Release`, `Security audit`, and (when
  cross-platform-relevant paths changed) `Cross-Platform Verification` run.
  Successful release assets stay immutable — a re-dispatch never mutates an
  already-published tag.
- Final published-artifact verification is extended on every Supported
  platform: RISC-V64 and LoongArch64 assets are re-verified under direct
  user-mode QEMU on an Actions host, FreeBSD on a native VM, and the new
  `post-release-verify-native` matrix re-verifies the actual GitHub Release
  Windows runtime asset on native Windows x86_64 and Windows ARM64 runners via
  the existing `host_smoke.py`.
- `move-index-pointer` now waits on the PM adapter published-artifact host
  smoke and the native post-release verify before promoting the candidate or
  stable index pointer, so a failed adapter or native verification can never
  be promoted.

## [1.2.0-rc.2] - 2026-08-17

### Changed

- The release-index schema advances to v3 (`RELEASE_INDEX_SCHEMA_VERSION = 3`)
  for the 1.2.0 Stable line. Linux runtime/adapter artifacts now record the
  libc variant explicitly in `targetLibc` (`gnu`/`musl`) so GNU and musl
  builds of the same Linux OS+arch are disambiguated deterministically,
  independent of the stable artifact `id`. Non-Linux targets (macOS, Windows,
  FreeBSD) omit `targetLibc`. v3 is a *versioned* schema: a 1.1.0 reader
  (whose validator requires schemaVersion == 2) rejects the RC2 index
  fail-closed at the schema boundary rather than naive-matching both Linux
  builds to the same OS+arch and failing ambiguously.


## [1.2.0-rc.1] - 2026-08-16

### Added — OpenWrt Performance Manager decision adapter

- New Preview crate `rill-pm-adapter`: a Unix-domain-socket decision host that
  speaks the independent `pm-rill-shadow` v1 contract (deliberately distinct
  from the Rill Runtime IPC API v1/v2/v3). It implements `status` /
  `observe` / `outcome` operations, context-partitioned + goal-partitioned
  models, a bounded decision ledger with strict validated-outcome rejection
  rules (unknown / duplicate / action-mismatch / context-mismatch /
  generation-mismatch / expiry / non-validated / non-finite reward), bounded
  persistent state with restart recovery, and model health reporting. The
  adapter is advisory-only: it never mutates OpenWrt / UCI / sysctl / ethtool
  state. Fail-closed framing: oversized NDJSON frames close the connection.
- The adapter is cross-built by CI (`build-pm-adapter-cross`) for the musl
  Linux targets PM consumes (`linux-x86_64-musl`, `linux-aarch64-musl`) and
  published as a distinct `pm-adapter` release-index artifact kind carrying
  `pmAdapterProtocolVersion` 1.
- New independent released-adapter host smoke (`adapter-host-smoke`) verifies
  the published binary's size/SHA-256 against the signed index, starts it on a
  fresh Unix-domain socket, and exercises a real `pm-rill-shadow` v1
  round-trip plus fail-closed negative cases (wrong contract, wrong protocol
  version, oversized frame).

### Changed

- The release-index schema stays at v2 (`RELEASE_INDEX_SCHEMA_VERSION = 2`) to
  preserve the 1.x Stable wire contract. GNU and musl Runtime assets of the
  same Linux OS+arch are distinguished by the stable artifact ``id``
  (`rill-runtime` vs the new `rill-runtime-musl`) rather than a new schema
  field. The `linux-x86_64-musl`, `linux-aarch64` (GNU), and
  `linux-aarch64-musl` Runtime assets are now included in the release
  pipeline, built and real-executed inside target-architecture
  Docker/QEMU containers on GitHub Actions. Existing assets
  (`linux-x86_64` GNU, `macos-aarch64`, `windows-x86_64`,
  `windows-aarch64`) are unchanged.

## [1.1.0] - 2026-08-02

### Changed

- The Stable crates advance additively to `1.1.0`; Preview workspace crates
  advance to `0.15.0`. No 1.0 API, state, IPC V1/V2, or WIT v1 contract is
  removed or rewritten.

### Added — explainable and high-performance contextual decisions

- `LinUcb` now exposes per-arm exploitation, alpha-adjusted exploration bonus,
  and total score through additive inherent methods, plus a deterministic
  lowest-index tie-break for replay. The existing randomized `select` contract
  and frozen serde fields are unchanged.
- Stable LinUCB scoring now uses positive-definite Cholesky solves instead of
  explicitly forming an inverse, exposes a bounded condition indicator, and
  rejects a numerically degenerate update before commit.
- Preview `LinUcbFast` maintains inverse matrices with Sherman-Morrison for
  `O(d^2)` scoring and updates. Cross-implementation tests constrain score
  drift; a dedicated Criterion benchmark covers 2/8 arms and 8/32/128
  features. Its serde state is explicitly Preview.

### Added — portable drift state and consensus

- Added Stable versioned portable continuity DTOs for Page-Hinkley, ADWIN and
  KSWIN. Restore requires exact configuration matching and validates schema
  version, finite values, counters, window bounds and detector invariants.
  Direct detector serde layouts remain Preview.
- Added `DriftConsensus`, a bounded Preview-state vote combiner with warning
  and drift thresholds, consecutive confirmation, clear hysteresis, warming,
  incomplete-data handling, cooldown, bounded event merging, generation
  context, explainable results, reset, serde and failure-atomic updates.

### Added — bounded online distribution statistics

- Added constant-memory `P2Quantile` and bounded multi-quantile
  `P2Quantiles`, including bootstrap exactness, state validation, serde
  continuity, randomized offline-rank comparisons and explicit documentation
  that P-square has no deterministic worst-case rank-error bound.
- Added exact `ClippedMean` for fixed caller-provided bounds.
- Added Preview `RollingMedianMad`: exact median and MAD within a bounded FIFO
  window, an explicit warm-up threshold, modified robust z-scores with a
  first-class zero-MAD result, strict restored-state validation, randomized
  offline-reference and adversarial contamination tests, and explicit
  `O(1)` update / `O(W log W)` query limits. This replaces the rejected
  lifetime double-P² prototype without reviving its contamination weakness.

### Added — delayed feedback, identity, and weighted learning

- Added a bounded, caller-clocked `DecisionLedger` with exact registration
  replay, strict delayed-feedback validation, explicit cleanup, tombstones,
  failure-atomic LinUCB integration, serde validation, and capacity limits.
- Added deterministic `FeatureSchema` hashing and algorithm/model descriptors
  with bounded strings, ordered features, canonical metadata, golden fixtures,
  and explicit schema-compatibility checks.
- Added separate weighted statistic/regression/classification traits and
  implementations for mean, population variance, EW mean, MAE, MSE, linear
  regression, logistic regression, and pipelines. Frozen unweighted traits and
  persisted fields are unchanged.

### Added — Preview runtime, replay, and Python tooling

- Added independent IPC V3 request/response types for handshake, health,
  observe, decide, feedback, inspect, snapshot, and reset. V1/V2 types and
  fixtures are unchanged; V3 has its own strict fixtures and JSON Schema.
- Added the Preview `rill:handler@2.0.0` stateful WIT world and a no-WASI
  Wasmtime host. The runtime owns bounded JSON state, schema version, checksum,
  generation, timeout/trap handling, and atomic commits. WIT v1 is unchanged.
- Added a bounded deterministic decision-replay harness with delayed/out-of-
  order outcomes, checkpoint restore, generation/schema/drift boundaries,
  reward/regret/baseline/latency summaries, and replay digests.
- Added Preview Python LinUCB score/replay bindings and dependency-free
  JSON/CSV, score-curve, drift-timeline, latency, baseline, and offline
  quantile/median/MAD/modified-z inspection helpers. Python remains outside
  the production Runtime.






## [1.0.0] - 2026-07-28

This is the first stable 1.x release. It promotes the four Stable crates from
`1.0.0-rc.6` without changing their frozen Rust API, state schemas, IPC
protocol, WIT ABI, pack formats, runtime CLI, default features, or MSRV.
Preview crates remain at `0.13.0`.

### Added — stable compatibility contract

- The Stable group is `rill-ml`, `rill-runtime`, `rill-runtime-protocol`, and
  `rill-handler-api`. The 1.x compatibility surface and additive-change rules
  are defined in `STABILITY.md` and enforced by public-API baselines,
  `cargo-semver-checks`, cross-version state fixtures, WIT checks, and
  protocol/runtime integration tests.
- The published runtime path includes signed model and handler packs, signed
  release indexes, Linux x86_64, Windows x86_64, and macOS Apple Silicon
  runtime assets, plus immutable versioned URLs and SHA-256 metadata.

### Changed — verified stable promotion

- The release workflow publishes an immutable `v1.0.0` release, runs an
  independent external-host smoke against the public release assets, and only
  then advances `local-ai-stable`. The smoke verifies index and package
  signatures, asset hashes and sizes, IPC v2 handshake identities, health, a
  real invoke result, the frozen `invalidJson` error code, and clean shutdown.
- The package dry-run is driven by `release-plan.toml` and verifies only the
  four Stable crate archives. Unpublished internal 1.0 dependencies are
  patched to the exact checked-out tag during verification, so a first stable
  release does not depend on registry indexing from an earlier failed attempt.
- Preview crates, including `rill-ml-python`, retain version `0.13.0` and are
  not promoted to 1.0 by this release.

### Security — macOS delivery policy

- The official macOS Apple Silicon runtime is intentionally allowed to be
  unsigned and non-notarized. Missing Apple Developer ID credentials never
  block a stable release. The release includes `*.unsigned.json` metadata and
  first-launch authorization guidance; model, handler, and release-index
  cryptographic verification remains mandatory.

## [1.0.0-rc.6] - 2026-07-28

### Added — cargo-semver-checks integration

- The four Stable crates (`rill-ml`, `rill-runtime-protocol`,
  `rill-handler-api`, `rill-runtime`) are now checked against the
  `1.0.0-rc.5` baseline using `cargo-semver-checks` in CI. Unlike the
  existing `cargo-public-api` text baseline (which is kept for
  human-readable drift review), `cargo-semver-checks` performs a
  rigorous lints-based analysis that catches breaking changes simple
  text comparison would miss (e.g. tightening trait bounds, deleting
  enum variants, changing function signatures, removing public fields).

### Added — Stable state schema whitelist

- `STABILITY.md` now explicitly lists the 26 Stable state schema types
  covered by the 1.x state-freeze contract, each with full
  cross-version fixture coverage (`v0.13.0/` + `v1/`) and a declared
  validation mode (`validate_state` / `deserialize` / `derive`).
  Preview state schema types (12 types including drift detectors,
  `LogisticRegression`, `LastValueRegressor`, etc.) are explicitly
  listed as not covered by cross-version fixture coverage.
- `state-schema-manifest.toml` is the authoritative source of truth
  for the Stable/Preview state schema classification.
- `scripts/check_state_fixture_coverage.py` validates in CI that
  every Stable type has fixtures, implements the declared validation
  mode, and has no duplicates or overlap with the Preview group.

### Added — release-plan-driven publishing

- The Release workflow's `publish` job now reads the Stable crate
  publish list from `release-plan.toml` via
  `scripts/parse_release_plan.py`, replacing the previous hardcoded
  list that included Preview crates and relied on crates.io returning
  "already exists" to skip them. Only the Stable group is published;
  Preview crates are never published by an RC tag.

### Changed — macOS unsigned policy (permanent)

- macOS aarch64 runtime is now always compiled and uploaded,
  regardless of whether Apple Developer ID secrets are configured.
  The previous behaviour skipped the macOS build entirely when
  secrets were absent, causing the release index to omit macOS
  support. The new behaviour always builds, skips codesign/
  notarization when secrets are absent, uploads the unsigned binary
  with a sidecar metadata file (`*.unsigned.json`), and includes
  the macOS runtime entry, size, and SHA-256 in the signed candidate
  index. The sidecar, rather than the frozen index schema, records
  `codeSigning: "unsigned"` and `notarization: false`.
- This is a permanent project decision that applies to all releases
  (RC, candidate, and final stable). The absence of Apple Developer
  ID is never a blocking condition for any release. See
  `STABILITY.md` § macOS unsigned policy for details.

### Fixed — malicious state tests

- `tests/state_fixtures.rs`: negative tests that previously had no
  assertions (`let _ = result;`) now use `assert!(result.is_err())`
  to actually verify that invalid states are rejected. New negative
  tests cover non-finite values, dimension mismatches, encoder
  mapping inconsistency, bandit arm count mismatch, and buffer
  capacity overflow.

## [1.0.0-rc.5] - 2026-07-28

### Fixed

- `ReleaseIndexPayload::validate_shape` in `rill-runtime-protocol` rejected
  the `candidate` release channel, causing `rill-pack sign-index` to fail
  with "unsupported release channel" when publishing a prerelease index.
  The validator now accepts both `stable` and `candidate` channels, matching
  the two-channel model documented in `STABILITY.md`.

## [1.0.0-rc.4] - 2026-07-28

### Fixed

- `scripts/build-release-index.py` failed with `FileNotFoundError` when a
  platform runtime asset was intentionally skipped by the release workflow
  (e.g. macOS builds skipped due to missing Apple Developer ID secrets). The
  script now skips missing platform assets instead of aborting, so the
  release index does not claim support for a platform whose asset was not
  produced — matching the skip behaviour documented in `STABILITY.md`.

## [1.0.0-rc.3] - 2026-07-28

### Fixed

- The release workflow failed hard when Apple Developer ID secrets were not
  configured, blocking Linux and Windows assets behind a macOS-only secret
  requirement. The workflow now detects missing secrets, skips the macOS build
  with a warning, and the release index does not claim macOS support for that
  release. `strategy.fail-fast` is now `false` so a skipped macOS matrix entry
  does not abort the whole release. `STABILITY.md` documents the skip behaviour.

## [1.0.0-rc.2] - 2026-07-28

### Fixed

- `cargo package` failed for `rill-runtime` because the `wasmtime::component::bindgen!`
  macro referenced `../rill-handler-api/wit/rill-handler.wit`, a relative path that
  does not exist inside the packaged tarball. The WIT source is now copied to
  `crates/rill-runtime/wit/rill-handler.wit` and the macro path updated to
  `wit/rill-handler.wit`. `scripts/check_wit_abi.py` now also verifies the copy
  stays byte-identical to the canonical source, preventing silent drift.

## [1.0.0-rc.1] - 2026-07-28

This is the first 1.0 release candidate. It finalises the API, state format,
IPC, WIT ABI, runtime product contract, and release channel behaviour that
will remain compatible throughout the 1.x cycle. The Stable group
(`rill-ml`, `rill-runtime`, `rill-runtime-protocol`, `rill-handler-api`)
advances to `1.0.0-rc.1`; the Preview group stays at `0.13.0`. The
`local-ai-stable` pointer is intentionally untouched and continues to point
at `0.13.0`; the new `local-ai-candidate` pointer tracks this RC.

### Added — Stability policy

- `STABILITY.md` defines the 1.0 compatibility surface: Stable vs Preview
  crates, Rust public API freeze, serde model state schema, IPC v1/v2 wire
  schema, WIT ABI v1, model/handler pack formats, runtime CLI, default
  features, MSRV (1.94.0), platform support, deprecation policy, 1.x
  breaking-change policy, and the stable/candidate release channels.
- `release-plan.toml` is the single machine-readable source of truth for
  the Stable/Preview version split. Stable crates inherit
  `1.0.0-rc.1` from `[workspace.package].version`; Preview crates pin
  `0.13.0` locally.

### Changed — Rust public API finalisation

- Dual module paths (`pub mod foo; pub use foo::*;`) collapsed to a single
  official re-export path. Implementation modules under `models`, `optim`,
  `loss`, `stats`, `preprocessing`, `metrics`, `drift`, `bandit`,
  `diagnostics`, and runtime `handler`/`package`/`server` are now
  `pub(crate)`; only the top-level re-exports are the public contract.
  `scripts/check_runtime_public_api.py` proves via an external compile-fail
  crate that internal paths cannot be imported from downstream code.
- `#[non_exhaustive]` added to all non-versioned, extensible enums
  (`RillError`, `Optimizer`, `RegressionLoss`, `HandlerLoadError`,
  `ModelPackError`, `HandlerPackError`, `ArchiveError`, `ReleaseIndexError`,
  `InvokeErrorKind`, `EngineResponse`, `DriftAction`, `DriftLevel`,
  `WarmupState`, `Confidence`, `NewFeaturePolicy`) and to all extensible
  `*Config` structs. Versioned wire-schema types (`RuntimeRequest`,
  `RuntimeResponse`, `RuntimeResponseV2`) are intentionally NOT marked
  `#[non_exhaustive]`; they evolve by introducing a new versioned type.
- Examples and tests updated to construct `*Config` structs via
  `Default::default()` + field assignment rather than struct literals,
  so adding a field in a future 1.x release is not a breaking change.

### Added — Versioned model state freeze

- `ValidateState` trait unifies per-type state validation (dimensions,
  finite values, non-negative counts, optimizer parameter counts, FTRL
  `max_features`, encoder mapping consistency, pipeline dimensions, bandit
  arm state, drift detector buffer).
- `Snapshot::into_model_with_validation(validate)` is the required restore
  path at trust boundaries. `Snapshot::into_model()` remains available for
  trusted state and is documented as such.
- Python and WASM `from_json` now go through `Snapshot::from_json_validated`,
  which enforces `MAX_SNAPSHOT_JSON_BYTES` before deserialization and runs
  `ValidateState` on the restored model. Untrusted `from_json` input that
  exceeds the size limit, contains non-finite values, declares inconsistent
  dimensions, or supplies an unknown `format_version` is rejected atomically.
- Cross-version state fixtures committed under `tests/fixtures/state/`:
  `v0.13.0/` (representative state produced by the immutable `v0.13.0` tag)
  and `v1/` (state produced by this RC). CI loads every fixture on every
  run; a schema bump requires a migration note in `CHANGELOG.md` and a new
  fixture set.

### Added — IPC v1/v2 freeze

- Golden IPC fixtures committed under
  `crates/rill-runtime-protocol/tests/fixtures/v1/` and `v2/` covering
  handshake, health, invoke, result, and error for both protocol versions.
  Each fixture is round-tripped (fixture → typed → JSON → normalised →
  fixture) and rejects unknown fields.
- Stable error code constants in `rill-runtime-protocol::error_code`:
  `invalidJson`, `invalidRequestId`, `incompatibleApiVersion`,
  `invalidClientIdentity`, `unsupportedCapability`, `noInvokeHandler`,
  `handlerTimeout`, `handlerTrap`, `handlerOutputTooLarge`,
  `handlerInvalidOutput`, `handlerInternalError`. Existing codes are frozen
  and additive; new codes may be added in 1.x but existing codes are never
  renamed.

### Added — WIT ABI v1 freeze

- `scripts/check_wit_abi.py` verifies that the four independent declarations
  of the WIT ABI version agree (canonical WIT source, `rill-handler-api`
  Rust constants, `rill-runtime-protocol` constant) and that the normalised
  WIT source SHA-256 matches the frozen value
  (`108a68dfd6bcf86e3b63ad630508b2bbf407d00e8634067366e53dbc257cc90c`).
- Prebuilt v1 component fixture
  (`crates/rill-runtime/tests/fixtures/handler-v1-component.wasm`, built
  from the `v0.13.0` tag) committed to the repository. Its SHA-256
  (`6cfb4bf2eac5d0d5a4644c56f58b2fd679fc989893bc381a8ae31b410852011b`)
  is verified before each load. CI never rebuilds this fixture from
  current WIT, so a WIT regression is detected when the current runtime
  can no longer load the historical component.
- `tests/wit_v1_component.rs` exercises the full
  `metadata → configure → invoke` lifecycle against the prebuilt
  component, packs it into a signed `.rillhandler`, and loads it back
  through the standard handler-pack loader.

### Changed — Runtime product contract

- `rill-runtime` now ships with `default = ["wasm"]` so that
  `cargo install rill-runtime` matches the official GitHub release binary
  behaviour. Builds that omit the feature must document that
  `.rillhandler` packs cannot be loaded.
- The implicit fallback to the deprecated built-in `linear-regression`
  handler has been removed. When neither `--handler` nor
  `--builtin-handler` is passed, the runtime fails to start with
  `MissingHandlerOption`. The explicit
  `--builtin-handler linear-regression` path is retained as a
  compatibility option.
- `--model-trust-key` is now the primary CLI parameter name; `--trust-key`
  is kept as a deprecated alias for 1.x compatibility. Help text and docs
  use the primary name only.
- macOS official release assets are Apple Silicon only and must be
  codesigned. The release workflow fails when signing secrets are missing
  rather than silently publishing unsigned assets.

### Added — Candidate release channel

- `scripts/build-release-index.py` gains a `--channel` argument
  (`stable` | `candidate`). The channel value is recorded in the signed
  payload so downstream clients can reject a candidate index when they
  expect a stable one.
- `pipeline.yml` detects prerelease versions (contains `-` after the
  patch number), passes `--prerelease` to `gh release create`, uploads
  `candidate-index.json` to the `local-ai-candidate` pointer release,
  and never touches `local-ai-stable`. Stable releases update only
  `local-ai-stable`.
- The SemVer validation regex in `pipeline.yml` now accepts standard
  prerelease tags (`^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$`).
- The Python crate remains in the Preview group and is not published to
  PyPI during the 1.0 RC cycle.

### Added — CI gates

- WIT ABI v1 consistency check (`scripts/check_wit_abi.py`) in the
  `wasm-handler` job.
- Prebuilt WIT v1 component compatibility test (`cargo test --test
  wit_v1_component`) in the `wasm-handler` job.

### Changed — Documentation

- `STABILITY.md` is the single source of truth for the 1.x compatibility
  surface. `SECURITY.md`, `CONTRIBUTING.md`, `README.md`, `README.en.md`,
  and `ROADMAP.md` now cross-reference it and no longer describe the
  project as `0.x — Experimental`.

## [0.13.0] - 2026-07-27

This release closes the sixth-stage minimal final closeout
([`RILL_ML_TRAE_SIXTH_STAGE_MINIMAL_FINAL_CLOSEOUT_PROMPT.md`](RILL_ML_TRAE_SIXTH_STAGE_MINIMAL_FINAL_CLOSEOUT_PROMPT.md))
on top of the 0.12.0 baseline. It completes the first fully-green
Release workflow verification after the fifth-stage PyPI visibility
fix, and adds two runtime verification hardenings that the fifth-stage
checks could not cover.

### Fixed — Release workflow

- Fixed the PyPI visibility-check Python heredoc indentation so a
  successful upload is correctly verified through the PyPI JSON API.
  The fix itself landed on `main` as part of the fifth-stage follow-up
  (`af8fb54`); this release is the first one to go through the full
  Release workflow with the fix in place, so the `Verify PyPI release
  is visible` step now actually succeeds.

### Fixed — Runtime verification

- Added an external-crate compile-fail check
  (`scripts/check_runtime_public_api.py`) proving the ticker test probe
  (`active_epoch_ticker_count`) is not part of the public API, including
  `#[doc(hidden)] pub` regressions that the previous rustdoc grep
  cannot detect. A companion smoke crate proves legitimate public API
  (`WasmInvokeHandler`) still compiles, so a broken dependency
  configuration cannot be misread as "probe invisible". Three Python
  unit tests cover the classification logic.
- Added a test-level timeout (`mpsc::channel` + `recv_timeout(15s)`) to
  the `metadata_loop_failure_restores_active_ticker_count` test so a
  ticker/epoch regression fails promptly instead of waiting for the CI
  job's 30-minute timeout. Worker panics are propagated via
  `resume_unwind` so the failure points at the actual panic site. The
  `baseline → baseline+1 → baseline` observation is preserved.

### Changed — CI

- The `wasm-handler` CI job gains a `Verify ticker probe is not
  externally accessible` step that runs the new compile-fail script.
  The existing rustdoc grep is kept as an auxiliary `rendered-doc smoke
  check` but is no longer the primary public-API proof.

### Changed — Audit and release consistency

- The sixth-stage audit chapter (§0.15) records the five blocking
  issues (6-A-01 / 6-A-02 / 6-B-01 / 6-B-02 / 6-C-01), their fixes,
  and the full cross-channel consistency verification for 0.13.0. The
  stale fifth-stage `wasm-pack: CI 未显式安装` record is corrected to
  reflect the actual workflow (`Install wasm-pack` / `Record resolved
  wasm-pack version` / `wasm-pack test --node`).

## [0.12.0] - 2026-07-27

This release closes the fifth-stage release-consistency and final
acceptance work
([`RILL_ML_TRAE_FIFTH_STAGE_RELEASE_CONSISTENCY_FINAL_PROMPT.md`](RILL_ML_TRAE_FIFTH_STAGE_RELEASE_CONSISTENCY_FINAL_PROMPT.md))
on top of the 0.10.0 baseline. It bundles the four fourth-stage
final-acceptance fixes (which were never published through a clean
release channel) together with the fifth-stage release-consistency
work that makes the ticker lifecycle tests actually execute in CI,
adds a real public-API leak check, and re-publishes the project so
that GitHub and PyPI carry the same code at the same version.

### Changed — Breaking

- The `#[doc(hidden)] pub fn active_epoch_ticker_count()` accessor that
  briefly existed in `0.10.0` has been removed. `#[doc(hidden)]` does
  not make an item private — it only hides it from rendered docs — so
  the accessor was a public API. The replacement is a `#[cfg(test)]`
  private `fn` inside `handler::wasm::tests`, which internal unit tests
  can reach through `super::*` but external crates (and the published
  crate) cannot see. Downstream code that mistakenly imported the
  accessor must delete the import; there is no replacement. Per the
  project's stated 0.x SemVer convention (public API may change
  between minor versions), this deletion is a minor bump.
- Project's own version numbers continue to follow the repository
  rule: they must not contain the digits `4` or `11`. `0.11.0` is
  therefore skipped; the next version after `0.10.0` is `0.12.0`.
  This rule applies only to project release tags / `Cargo.toml`
  version / installer filenames; third-party dependency versions are
  unaffected.

### Fixed — Core ML serde trust boundary (fourth-stage)

- `FtrlRegressor` / `FtrlClassifier` (`src/models/ftrl.rs`): the top-level
  `validate_invariants()` now rejects deserialised state where
  `params.len() > max_features` when `max_features` is `Some(_)`. Without
  this check, a malicious payload could supply pre-built params with more
  entries than the configured `max_features` cap allows, bypassing the
  dynamic-growth contract. `params.len() == max_features` and
  `params.len() < max_features` remain legal; `max_features == None`
  remains unbounded. Six new serde tests cover regressor and classifier
  across reject-above / accept-equal / allow-unbounded scenarios. Existing
  config-aware `weight_checked` / `intercept_weight_checked` validation
  is unchanged.

### Fixed — Runtime ticker lifecycle test-only instrumentation (fourth-stage)

- `handler/wasm.rs` / `tests/wasm_handler.rs`: the ticker lifecycle probe
  has been migrated out of the production public API. The 0.10.0 design
  exposed `#[doc(hidden)] pub fn active_epoch_ticker_count()` so that
  integration tests (a separate crate) could reach the counter; this was
  a leak, because `#[doc(hidden)]` only hides the item from rendered docs
  and does not make it private. The matching `ACTIVE_EPOCH_TICKERS`
  `AtomicUsize` and its `fetch_add` / `fetch_sub` ops also ran in the
  production ticker hot path.

  The 0.12.0 design:
  - Gates `ACTIVE_EPOCH_TICKERS`, the `fetch_add` / `fetch_sub` ops in
    `EpochTicker::start`, and the `active_epoch_ticker_count` accessor
    behind `#[cfg(test)]`. The counter does not exist in release builds,
    in CI builds that link the library as a dependency (integration
    tests), or in the published crate.
  - Removes the public `active_epoch_ticker_count()` accessor entirely.
  - Moves the lifecycle tests
    (`metadata_loop_failure_restores_active_ticker_count`,
    `normal_handler_drop_restores_active_ticker_count`) into the
    library's internal `#[cfg(test)] mod tests` in `src/handler/wasm.rs`,
    which reaches private items directly through `super::*`.
  - Renames the previous `ticker_probe_is_not_in_public_api` test to
    `ticker_probe_is_available_to_internal_tests` to accurately describe
    what it asserts. The actual public-API leak check now lives in CI.

  The production ticker hot path now performs zero atomic operations for
  instrumentation. Timeout, fuel, epoch, and resource limits are
  unchanged, and `WasmInvokeHandler`'s external behaviour is unchanged.

### Fixed — CI execution of ticker lifecycle tests (fifth-stage)

- `.github/workflows/pipeline.yml`: the dedicated `wasm-handler` job
  builds the WASM fixtures but previously only ran the *integration*
  tests (`--test wasm_handler` / `--test runtime_process`). The
  internal `#[cfg(test)] mod tests` inside `handler/wasm.rs` — which
  directly observe the test-only `ACTIVE_EPOCH_TICKERS` counter — were
  silently skipped because they live behind `--lib` and the integration
  test runs did not propagate the fixture env vars to them. A new
  `Run internal WASM ticker lifecycle tests` step runs the three
  lifecycle tests explicitly with `--exact --nocapture` so each test
  name appears verbatim in the CI log.
- A new `RILL_RUN_WASM_FIXTURE_TESTS=1` env var is the explicit
  execution gate. When set, fixture absence panics instead of
  printing "skipping" and returning success. The dedicated
  `wasm-handler` CI job sets this var after building the fixtures;
  the regular workspace `cargo test` job (which does not build the
  fixtures) still skips gracefully.

### Fixed — Strengthened metadata-loop lifecycle test (fifth-stage)

- `crates/rill-runtime/src/handler/wasm.rs`: the
  `metadata_loop_failure_restores_active_ticker_count` test now spawns
  the constructor in a worker thread so the main thread can observe
  `count == baseline + 1` *during* construction. The previous
  synchronous flow could pass even if the ticker never started: by the
  time `WasmInvokeHandler::new()` returned `Err`, the constructor had
  already joined the ticker thread, leaving the counter at baseline.
  The strengthened flow is `baseline → spawn worker → observe
  baseline + 1 → worker returns Err → observe baseline`, which directly
  proves the ticker was started and then joined on failure. The test
  does not need `LoadedHandlerPack` to be `Clone` — it wraps the pack
  in `Arc<LoadedHandlerPack>` and shares the `Arc` with the worker.

### Fixed — Real public-API leak check (fifth-stage)

- `.github/workflows/pipeline.yml`: a new `Verify ticker probe is
  absent from public API docs` step runs `cargo doc --locked -p
  rill-runtime --features wasm --no-deps` and `grep`s the rendered
  rustdoc for `active_epoch_ticker_count` / `ACTIVE_EPOCH_TICKERS`.
  A match fails the build, so a future refactor that re-exposes the
  test probe as `pub` cannot slip through CI.

### Fixed — Release consistency (fifth-stage)

- `0.10.0` on GitHub and `0.10.0` on PyPI referenced different code
  states: the original PyPI upload predated the fourth-stage fixes; the
  GitHub `v0.10.0` tag was later re-created at the fourth-stage HEAD
  (`0f9923a`); PyPI rejected the re-upload because the wheel filename
  was already taken. `0.12.0` resolves the inconsistency — the new
  tag, GitHub Release, PyPI wheel, CHANGELOG section, and signed
  stable-index all correspond to the same commit.

### Fixed — Audit report accuracy (fourth-stage + fifth-stage)

- `docs/audits/RILL_ML_LATEST_AUDIT_AND_OPTIMIZATION.md`: corrected the
  Multinomial tolerance description to match the actual code constants
  `MULTINOMIAL_TOTAL_ATOL = 1e-9` and `MULTINOMIAL_TOTAL_RTOL = 1e-6`
  (the previous text claimed both were `1e-9`). Added §0.13 fourth-stage
  acceptance closeout with an independent 4-issue problem table, current
  HEAD / `origin/main` SHA, ahead/behind status, actual locally-resolved
  tool versions, and CI verification status. §0.14 records the
  fifth-stage 8-issue problem table, the new ticker CI execution flow,
  the strengthened metadata-loop test, the real public-API rustdoc
  check, and the release-consistency closeout. The historical §0.13.11
  35-item report is explicitly marked as a fourth-stage pre-re-publish
  snapshot whose acceptance items #31 / #32 / #33 were invalidated by
  the user-authorized `v0.10.0` re-publish.

## [0.10.0] - 2026-07-26

This release closes the third-stage audit
([`RILL_ML_TRAE_THIRD_STAGE_FINAL_CLOSEOUT_PROMPT.md`](RILL_ML_TRAE_THIRD_STAGE_FINAL_CLOSEOUT_PROMPT.md))
on top of the 0.9.0 baseline. It tightens the serde trust boundary for the
remaining core models, makes the WASM ticker lifecycle directly observable
from tests, and corrects the audit report and CHANGELOG to match the actual
repository state. The runtime's public `InvokeErrorKind` enum is expanded
and marked `#[non_exhaustive]`; both changes are breaking for downstream
exhaustive `match` arms and therefore require a minor bump.

### Changed — Breaking

- `InvokeErrorKind` (in `crates/rill-runtime/src/server.rs`): the
  previously exhaustive 0.9.0 enum had six variants
  (`Internal`, `Timeout`, `Trap`, `OutputTooLarge`, `InvalidOutput`,
  `ExecutionFailed`). Three new variants are added for direct WIT
  `handler-error` mapping — `InvalidModel`, `InvalidInput`,
  `UnsupportedCapability` — and the enum is now marked
  `#[non_exhaustive]`. Existing exhaustive `match` arms on
  `InvokeErrorKind` must add a wildcard `_` arm. The stable IPC error
  codes are unchanged: the four guest-reported variants
  (`InvalidModel`, `InvalidInput`, `UnsupportedCapability`,
  `ExecutionFailed`) all still serialize to `handlerInternalError` for
  v1/v2 wire compatibility.

### Changed — Non-breaking

- `pipeline.yml` / `scripts/requirements-dev.txt`: the build-tool
  versions are now expressed as a **compatible range** rather than an
  exact pin. `wasm-pack` uses `~0.13.1`, `wasm-tools` uses `~1.254.0`,
  and the Python tools use `maturin>=1.14,<1.15` and
  `pytest>=8.4,<8.5`. CI and release share the same ranges, and CI
  records the actually-resolved versions after install. Patch upgrades
  are allowed so security fixes land without a manual bump; minor
  upgrades still require an explicit range bump and a re-run of CI.
- `StandardScaler::transform()` no longer runs a full `O(d)` `validate()`
  scan on each call. Internal state invariants are now guaranteed by the
  private constructor and the validated `Deserialize` impl, and re-checked
  only under `debug_assert!` inside `transform_into()`. The release path
  keeps only input dimension and output finiteness checks.

### Fixed — Core ML serde trust boundary

- `GaussianNaiveBayes` / `BernoulliNaiveBayes` / `MultinomialNaiveBayes`
  (`src/models/naive_bayes.rs`): replaced the derived `Deserialize` with
  a custom impl that constructs the model and calls a new
  `validate_invariants()` before returning. The validator rejects
  `feature_count == 0`, invalid `NaiveBayesConfig`, vector-length
  mismatches between `feature_count` and per-class stats, non-finite
  means/M2/feature sums, negative M2/feature sums, `samples_seen !=
  class_false.class_count + class_true.class_count`, per-feature counts
  that disagree with `class_count`, and Gaussian count=0/count=1 states
  whose mean or M2 is non-zero. Multinomial totals are checked against
  feature sums with an explicit relative/absolute tolerance. The
  `predict_proba` `Ok(NaN)` path is also rejected. Each `learn()` is
  atomic: any failure leaves the model completely unchanged. The
  `class_false_count + class_true_count` sum uses `checked_add` so a
  malicious payload with both counts near `u64::MAX` is rejected with
  `InvalidState` instead of panicking in debug or wrapping in release.
- `FtrlRegressor` / `FtrlClassifier` (`src/models/ftrl.rs`): added
  `FtrlParam::weight_checked(&config)` and
  `FtrlParam::intercept_weight_checked(&config)` which validate the
  numerator, denominator, and final quotient for finiteness and
  non-zero denominator, taking the L1 short-circuit and intercept
  cold-start `n == 0` paths into account. The top-level
  `validate_invariants()` now calls these for every stored param and
  the intercept, so a malicious payload whose `alpha`/`beta`/`l2`/`n`
  combination would underflow the denominator to zero is rejected at
  deserialisation rather than producing `Infinity` weights at
  `weights()`/`predict()` time. The pre-existing `n == 0 && z != 0`
  rejection and `checked_finite_add` final dot+intercept are retained.
- `R2` (`src/metrics/regression.rs`): the custom `Deserialize` now
  rejects `count == 0` states with non-zero `ss_res`, `mean_truth`, or
  `m2_truth`, and `count == 1` states with non-zero `m2_truth`.
  `count == 1` with non-zero `ss_res` remains legal (single-sample
  predictions may have residual error). The pre-existing finite and
  non-negative checks are retained.
- `StandardScaler` (`src/preprocessing/standard_scaler.rs`): the
  `validate()` path now rejects `count == 0` states with any non-zero
  mean or M2, and `count == 1` states with any non-zero M2. This
  matches the documented zero-sample contract (mean=0, scale=1, input
  returned unchanged) and closes the
  `counts=[0], means=[10], m2s=[1]` malicious-state gap. The release
  `transform()` hot path continues to skip the full scan.

### Fixed — Runtime ticker lifecycle observability

> **Fourth-stage update (superseded by 0.12.0)**: the
> `#[doc(hidden)] pub` accessor described below leaked a test probe
> into the public API. It has been removed and the probe migrated to
> `#[cfg(test)]`-only internal instrumentation. See the `[0.12.0]`
> section above for the full fourth-stage and fifth-stage closeout.

- `handler/wasm.rs` / `tests/wasm_handler.rs`: added a test-only
  `active_epoch_ticker_count()` accessor backed by an
  `AtomicUsize` counter that is incremented at epoch-ticker thread
  entry and decremented before thread exit. The `EpochTicker::drop`
  implementation joins the thread, so once `drop` returns the counter
  has been updated. Two new tests
  (`metadata_loop_failure_restores_active_ticker_count`,
  `normal_handler_drop_restores_active_ticker_count`) poll this counter
  with a bounded timeout, replacing the previous indirect test that
  could only prove a subsequent handler still works — not that the old
  ticker thread had actually exited. The accessor is `#[doc(hidden)]
  pub` so integration tests (a separate crate) can reach it without
  surfacing it in the documented API; `#[cfg(test)]` is not used
  because the library is not compiled with `--test` when integration
  tests link to it.
- `tests/wasm_handler.rs`: replaced the ticker-only `TICKER_TEST_LOCK`
  with a file-wide `WASM_TEST_LOCK` and a `wasm_test_guard()` helper.
  Every `#[test]` in the file now acquires the guard at entry, because
  each `WasmInvokeHandler` construction starts an `EpochTicker` thread
  that increments the global counter. Without file-wide serialisation
  the baseline captured by a ticker test could shift underneath it as
  parallel tests constructed or dropped their own handlers. The guard
  is recovered from poison so a panic in one test does not cascade via
  `PoisonError`. No production code changes. A follow-up commit added
  the missing guard to `wasm_handler_fuel_exhaustion_returns_timeout`,
  which was overlooked in the initial file-wide sweep — all 19 `#[test]`
  functions now acquire the guard.

### Fixed — Release tooling and policy

- `release_tag_policy.py`: compare `tag_sha` against `target_sha`
  BEFORE checking successful/active release state. A stale tag pointing
  at an old SHA but with an existing successful Release run is now a
  hard fail regardless of Release state, instead of being silently
  skipped.
- `handler/wasm.rs`: removed the `eprintln!` that printed the full
  guest-controlled detail before `InvokeError::with_detail()` could
  truncate it. Introduced a `HostLogSink` trait so tests can capture host
  log output. Detail is truncated to 4 KiB on a UTF-8 character boundary
  before any host log emission, and the IPC message never carries the
  detail. Each invoke error is logged exactly once.
- New test-only `test-metadata-loop-handler` WASM component whose
  `metadata()` loops forever. The CI `wasm-handler` job builds it as a
  WASM component and feeds it to integration tests that verify the
  epoch deadline fires and no ticker thread leaks.
- `README.md` / `README.en.md`: replaced the generic "bounded memory"
  claim with an accurate statement that fixed-dimension algorithms use
  bounded state, while dynamic-feature algorithms such as FTRL require
  `max_features` to be set for bounded state. The `max_features: None`
  default is unchanged.


## [0.9.0] - 2026-07-26

This release lands the full code audit
([`RILL_ML_TRAE_AUDIT_OPTIMIZATION_PROMPT.md`](RILL_ML_TRAE_AUDIT_OPTIMIZATION_PROMPT.md))
across core correctness, the runtime trust boundary, release reliability,
and documentation. Full report:
[`docs/audits/RILL_ML_LATEST_AUDIT_AND_OPTIMIZATION.md`](docs/audits/RILL_ML_LATEST_AUDIT_AND_OPTIMIZATION.md).

### Changed — Breaking: serde state format

- `Precision`, `Recall`, `F1Score`: added `samples_seen: u64` field so
  `samples_seen()` reports total observations (including true negatives)
  instead of `TP+FP` / `TP+FN` / `TP+FP+FN`. Old serde state missing this
  field is **rejected** because the true-negative count cannot be
  reconstructed from confusion counts alone. Callers must retrain or
  re-initialise these metrics after upgrading.
- `R2`: replaced `sum_truth` / `sum_sq_truth` fields with Welford-style
  `ss_res` / `mean_truth` / `m2_truth` / `count` to avoid catastrophic
  cancellation on large-offset, small-variance data (e.g.
  `y = 1_000_000_000_001/002/003`). Old serde state is **rejected**;
  callers must re-initialise `R2` after upgrading.
- `Metric` trait-level consistency tests added in `src/traits.rs` enforce
  that `N` successful `update` calls yield `samples_seen() == N` for every
  `Metric` implementation.

### Added — Core ML and pre-processing

- `FtrlConfig::max_features: Option<usize>` and
  `NewFeaturePolicy::{Reject, Ignore}` bound dynamic feature growth.
  New features in a single sample are pre-judged as a batch so a learn
  call never partially inserts. `Reject` is failure-atomic; `Ignore`
  updates only existing features. The default `None` preserves
  backwards compatibility; the module docs warn about unbounded growth
  on untrusted streams.
- `StandardScaler::transform_into` reuses a caller-provided buffer and
  fuses scale computation into a single pass. Public `transform()`
  stays API-compatible. `debug_assert!` retains dev-only invariants.
  Criterion bench covers `d = 8/32/128/1024`.
- Trait docs and classifier docs document the unified closed `[0, 1]`
  probability contract. `BinaryLogLoss` clips internally; `LogLoss`
  validates the `[0, 1]` range.

### Fixed — Core ML numerical safety

- FTRL `learn` now computes the next param state via `next_updated`
  before mutating any field. All intermediate products (gradient²,
  sigma, z-delta, dot product, prediction) pass through `ensure_finite`
  or `checked_finite_add`. `samples_seen` overflow is detected before
  state mutation. Single-learn failure leaves params, intercept,
  feature table, and counters unchanged.
- `Precision`, `Recall`, `F1Score`, `R2`, and `Accuracy` counters use
  `checked_increment`; overflow aborts the update without mutating
  state. Reset clears every counter.
- Serde deserialisers for `FtrlConfig`, `FtrlParam`, `FtrlRegressor`,
  `FtrlClassifier`, `R2`, `Precision`, `Recall`, `F1Score` reject
  non-finite values, negative accumulators, missing `samples_seen`,
  and inconsistent counters.

### Fixed — Runtime trust boundary

- `InvokeError` replaces string-prefix parsing with a typed
  `InvokeErrorKind` (`InvalidInput`, `UnsupportedCapability`,
  `ExecutionFailed`, `Timeout`, `Trap`, `OutputTooLarge`,
  `InvalidOutput`, `Internal`). The IPC client message is a fixed
  string; the host-side `detail` is truncated to 4 KiB on a UTF-8
  char boundary and never enters the message. Guest backtraces,
  model bytes, paths, and env vars are no longer exposed. v1 and v2
  wire codes are preserved.
- `WasmInvokeHandler` starts the `EpochTicker` RAII guard immediately
  after `Engine::new`, before `Component::new`, `metadata()`,
  `configure()`, or `invoke()`. Each stage resets fuel and epoch
  deadline independently. `Drop` signals the ticker thread and joins
  it within one tick. Mutex poison is mapped to `InvokeError::Internal`
  instead of panicking.
- WASM sandbox resource model is explicit and tested: no WASI, 64 MiB
  max memory, 10 000 max table elements, 1 MiB max I/O per call. The
  `ResourceLimiter` enforces memory and table caps with four boundary
  cases each. Docs accurately reflect "first version is serial call
  only".

### Fixed — Release reliability and supply chain

- `auto-release.yml` delegates tag dispatch/skip/fail decisions to the
  new `scripts/release_tag_policy.py`. Existing tags are immutable:
  when `tag_sha` differs from the successful CI head SHA the run fails
  loudly and asks the caller to bump the version. Tags are never moved.
  Stale releases are not retried as if they were the current main.
  11 Python tests cover every branch.
- `pipeline.yml` `publish-python` now treats a missing
  `MATURIN_PYPI_TOKEN` as a hard failure (Python bindings are part of
  the official release per README §模块总览) and verifies the uploaded
  version is visible on PyPI's JSON API within 60 s.
- Release tools are pinned: `wasm-pack` and `wasm-tools` use
  `cargo install --locked --version "^0.13"` / `"^1.0"`; `maturin`
  and `pytest` are pinned in the new `scripts/requirements-dev.txt`;
  the `rustsec/audit-check` Action is pinned to release commit
  `69366f33c96575abad1ee0dba8212993eecbe998`. `RUNTIME.md` documents
  the toolchain version sources.
- `security.yml` now triggers on `src/**`, `crates/**`, `handlers/**`,
  `scripts/**`, `models/**`, `.github/workflows/**`, `Cargo.toml`, and
  `Cargo.lock`. CodeQL runs `rust, python, actions`. Permissions are
  per-job minimum: default `contents: read`, CodeQL
  `security-events: write`, RustSec `checks: write, issues: write`.
- `archive.rs` replaces the integer-division compression-ratio check
  with `compressed.checked_mul(ratio)` to prevent truncation and
  wrap-around attacks. Four boundary tests cover exact boundary,
  one byte over, zero compressed size, and overflowing product.
- `scripts/build-release-index.py` and
  `scripts/update-model-release-index.py` enforce HTTPS-only release
  URLs, reject `file:`/`data:`/`javascript:`/`ftp:`/`blob:` schemes,
  empty hosts, embedded credentials, and fragments. `localhost` is
  rejected unless `RILL_ALLOW_LOCALHOST_URLS=1` is set explicitly.

### Changed — Documentation and version consistency

- README.md and README.en.md installation examples now use
  `cargo add rill-ml` / `cargo add rill-ml --features serde` instead of
  hardcoded `rill-ml = "0.x"` TOML lines, so the docs never drift from
  the canonical workspace version. Hardcoded test counts are replaced
  with a dynamic description pointing at `cargo test --workspace` and
  the CI summary.
- `scripts/sync_version.py` propagates the canonical
  `[workspace.package].version` to every static file: pyproject.toml,
  JSON manifests, excluded handler crates, ROADMAP status line,
  SECURITY supported-versions table, CHANGELOG skeleton, and
  `[workspace.dependencies]` internal version requirements. The script
  is idempotent and guards against README hardcoded versions. 14
  Python tests cover the simple and inline-table TOML forms.

## [0.8.1] - 2026-07-19

### Fixed

- Raise the sandboxed handler `invoke` fuel budget so valid payloads near the
  documented 1 MiB input limit can finish JSON decoding and execute. The
  independent five-second epoch deadline, memory cap, table cap, and I/O limits
  remain enforced.

## [0.8.0] - 2026-07-17

### Added — Security test coverage

- Handler package attack tests (`handler_package.rs`): 9 new tests covering
  tampered manifest/module/checksums/signature, unknown publisher key,
  directory entries, oversized files, compression bombs, and load-time
  module digest mismatch.
- WASM sandbox attack tests (`wasm_handler.rs`): 5 new tests covering
  traps, oversized output, invalid JSON output, infinite-loop timeout via
  epoch interruption, plus an echo-mode baseline. A new
  `handlers/test-malicious-handler/` fixture (controllable via model JSON
  `mode` field) supports these scenarios.
- Compatibility tests: `handler_pack_rejects_runtime_too_old`,
  `release_index_rejects_v1_schema`, `builtin_handler_deprecation_notice`,
  and cross-process WASM handler handshake.

### Changed — Version management centralisation

- The workspace version is now a single source of truth in
  `[workspace.package] version` of the root `Cargo.toml`. All workspace
  crates inherit via `version.workspace = true`; internal dependencies
  are declared once in `[workspace.dependencies]`.
- Handler source files and integration tests use `env!("CARGO_PKG_VERSION")`
  instead of hardcoded version strings.
- New `scripts/sync_version.py` propagates the canonical version to every
  static file (pyproject.toml, JSON manifests, excluded handler crates,
  ROADMAP, SECURITY, CHANGELOG skeleton) in one command.

### Fixed — Code quality (v2 comprehensive review findings)

- `server.rs`: removed dead `handlerCapabilityMismatch` branch in
  `map_invoke_error`; default fallback changed from `invokeFailed` to
  `handlerInternalError`.
- `wasm.rs`: trap classification now uses `downcast_ref::<Trap>()` instead
  of string matching; IPC error messages sanitised to avoid leaking
  wasmtime internals.
- `archive.rs`: added `max_compressed_total_bytes` (8 MiB) to
  `ArchiveLimits` to reject compression bombs.
- `rill-runtime-protocol`: capability duplicate detection rewritten to use
  `HashSet` (catches non-adjacent duplicates).
- WIT types moved from inside the `world` to a package-level `interface`
  block for cleaner ABI surface.
- `rill-ml` core: all counters migrated to `checked_increment`; Welford
  and on-demand computation paths use `checked_finite_add`/`ensure_finite`;
  Naive Bayes probability clamped to `(EPSILON, 1-EPSILON)`.
- `rillml-inspect`: MSRV constant updated to 1.94.
- `RUNTIME.md`: removed non-existent debug env var documentation;
  `--trust-key` now accepts `--model-trust-key` as an alias.
- `HANDLER-RFC.md` §3.2/§5/§6.1/§6.2: documented directory-entry handling,
  WASM stack size, built-in handler API version, and error mapping.
- `ROADMAP.md`: added v0.7 section and marked v0.1–v0.5 as completed.

## [0.7.2] - 2026-07-16

### Changed

- Stop publishing Intel macOS Runtime binaries. Official macOS releases and
  the signed stable index now contain Apple Silicon (ARM64) only.
- Keep Linux and Windows Runtime releases on x86_64.

## [0.7.1] - 2026-07-16

### Changed — Security: wasmtime 27 → 46

- Upgrade `wasmtime` from 27 to 46.0.1 (latest stable release track).
  Wasmtime 27 was not on a supported release line and carried 15 unpatched
  security advisories, including a Critical (CVSS 9.0) aarch64 sandbox
  escape (CVE-2026-34971, RUSTSEC-2026-0096). Wasmtime 46 is the current
  stable release track with all known advisories resolved.
- Bump workspace MSRV from 1.85 to 1.94 (wasmtime 46 requires Rust 1.94).
  Updated CI MSRV check, README, CONTRIBUTING, HANDLER-RFC, and
  THIRD_PARTY_NOTICES accordingly.
- No API changes required: the `rill-runtime` handler host code is
  source-compatible with wasmtime 46.

### Fixed — Release pipeline

- Merge `ci.yml` and `release.yml` into a single `pipeline.yml` following
  the mira-mouse pattern. CI jobs run on push/PR; release jobs run on
  `workflow_dispatch` (dispatched by Auto Release after CI succeeds).
  The tag-push trigger is intentionally omitted to avoid duplicate runs.
- Fix stable-index schema incompatibility that broke v0.7.0 release:
  `verify-index` failures on legacy v1 schema are now tolerated with a
  warning instead of failing the release.
- Fix `rill-pack create-handler` to auto-compute `moduleSha256` and
  `moduleSize` from the actual WASM module bytes. The source manifest
  template no longer needs to pre-contain these fields.
- Fix Python `SyntaxWarning` in `test_release_version.py` by using a raw
  string for the regex escape sequence.

## [0.7.0] - 2026-07-15

### Added — Pluggable WASM handler architecture

v0.7 transforms `rill-runtime` into a business-neutral general runtime that
loads signed WASM handler components. Handlers implement specific capabilities
via the WebAssembly Component Model; updating a handler no longer requires
recompiling or replacing the `rill-runtime` binary.

- **`rill-handler-api`** (`crates/rill-handler-api`): new crate defining the
  versioned WIT handler contract (`invoke-handler` world). Exports
  `HANDLER_API_VERSION = 1`. Handler authors depend on this crate for the
  canonical ABI; the runtime uses it for host-side bindings.
- **Handler package format** (`.rillhandler`): signed ZIP archive containing
  `manifest.json`, `handler.wasm`, `checksums.json`, and
  `META-INF/signature.ed25519`. Manifest declares handler id, version,
  handler API version, minimum runtime version, capabilities, and module
  SHA-256. Trust domain is separated from model packs — a model key cannot
  authorise a handler.
- **`rill-runtime::handler`** module: shared handler types
  (`HandlerIdentity`, `HandlerLoadError`), `effective_capabilities()`
  (intersection of model and handler capabilities), built-in
  `LinearRegressionInvokeHandler` (moved from `server.rs`), and
  `WasmInvokeHandler` (behind the `wasm` feature).
- **`WasmInvokeHandler`** (`crates/rill-runtime/src/handler/wasm.rs`):
  sandboxed WASM host adapter using Wasmtime 27. Enforces fuel budget,
  epoch interruption, memory/table limits, and I/O size caps. No WASI
  imports (no filesystem, network, environment, stdio, or process access).
  Verifies guest `metadata()` matches signed manifest before
  instantiation. Maps traps and timeouts to stable error codes.
- **IPC API v2**: `RUNTIME_API_VERSION` raised to 2. V2 handshake includes
  `handlerId`, `handlerVersion`, `handlerApiVersion`, and
  `effectiveCapabilities`. V1 responses omit handler fields entirely — the
  two wire schemas are separate types, not a single struct with `Option`
  fields. The runtime serves both v1 and v2 clients based on the request's
  `apiVersion`.
- **`EngineResponse`** internal type: captures all response data including
  handler identity; converted to `RuntimeResponse` (v1) or
  `RuntimeResponseV2` (v2) at the IPC boundary.
- **CLI handler options**: `rill-runtime serve` accepts `--handler
  <path.rillhandler>`, `--handler-trust-key KEY=HEX`, and
  `--builtin-handler linear-regression`. `--handler` and `--builtin-handler`
  are mutually exclusive. Default behaviour (no handler specified) falls
  back to built-in linear-regression with a deprecation warning.
- **`rill-pack` handler commands**: `create-handler --manifest
  handler-manifest.json --module handler.wasm --output example.rillhandler`
  and `inspect-handler --handler example.rillhandler --key-id KEY --public-key-hex HEX`.
- **Release index schema v2**: `RELEASE_INDEX_SCHEMA_VERSION` raised to 2.
  `ReleaseArtifactKind` gains a `Handler` variant (platform-independent,
  requires `handlerApiVersion` and `minRuntimeVersion`, no OS/arch fields).
  `build-release-index.py` supports `--handler-id`, `--handler-version`,
  and `--handler-min-runtime` arguments.
- **Shared archive skeleton** (`crates/rill-runtime/src/archive.rs`):
  common ZIP path validation, size limits, checksum verification, and
  Ed25519 signature logic extracted from `package.rs`. Both model and
  handler packs use the same safe-archive foundation with independent
  manifests and error types.
- **CI MSRV expansion**: MSRV 1.85 check now covers `rill-handler-api`,
  `rill-runtime-protocol`, and `rill-runtime` (default features). The
  `wasm` feature is tested on stable Rust across Linux, Windows, and
  macOS.

### Changed

- `models/example-default/manifest.json`: `runtimeApiVersion` 1 → 2,
  `minRuntimeVersion` 0.6.0 → 0.7.0, version 0.6.0 → 0.7.0.
- `LinearRegressionInvokeHandler` moved from `server.rs` to
  `handler/builtin.rs`; `LINEAR_REGRESSION_CAPABILITY` re-exported from
  crate root.
- `RuntimeEngine` now holds `handler_identity: Option<HandlerIdentity>`
  and `effective_capabilities: Vec<String>`. Capability checking uses
  effective capabilities when a handler is loaded.
- `scripts/build-release-index.py`: `RUNTIME_API_VERSION` 1 → 2,
  `RELEASE_INDEX_SCHEMA_VERSION` 1 → 2.
- `scripts/update-model-release-index.py`: schema and API version bumped
  to 2.
- `scripts/verify-release-assets.py`: `*.rillhandler` added to local file
  discovery glob patterns.
- Release workflow publishes `rill-handler-api` to crates.io and includes
  `*.rillhandler` in asset upload/download patterns.

### Compatibility notes

- IPC v1 clients (api_version=1) continue to work; responses omit handler
  fields. V2 clients receive full handler identity in the handshake.
- The built-in linear-regression handler remains available via
  `--builtin-handler linear-regression` and as the default when no handler
  is specified (with a deprecation warning).
- `MODEL_PACK_FORMAT_VERSION` remains 1; existing `.rillpack` files are
  forward-compatible.
- `HANDLER_PACKAGE_FORMAT_VERSION = 1` and `HANDLER_API_VERSION = 1` are
  introduced for the first time.
- The `wasm` feature is opt-in (`--features wasm`); default builds of
  `rill-runtime` do not pull in Wasmtime.
- Wasmtime 27 is pinned with minimal features: `cranelift`,
  `component-model`, `runtime`, `parallel-compilation`. No default
  feature set is used.

## [0.6.0] - 2026-07-15

### Added — Platform and ecosystem expansion

v0.6 widens RillML's reach without growing the core crate. Five new
independently-publishable crates live under `crates/` and depend on `rill-ml`
without changing its public API. The core library remains lightweight: no new
dependencies are introduced into `rill-ml` itself.

- **`rill-ml-tokio`** (`crates/rill-ml-tokio`): async Stream adapters that
  drive the same `predict → metric.update → learn` contract over
  `tokio_stream::Stream`. Core models stay synchronous; this crate provides
  `progressive_regress_stream` and `progressive_classify_stream`.
- **`rill-ml-arrow`** (`crates/rill-ml-arrow`): conversion helpers between
  Apache Arrow `RecordBatch` / `Float64Array` and RillML's `&[f64]` feature
  slices. Keeps DataFrame plumbing out of the core crate.
- **`rill-ml-polars`** (`crates/rill-ml-polars`): conversion helpers between
  Polars `DataFrame` and `(features, target)` sample pairs, plus a helper to
  append predictions as a new column.
- **`rillml-inspect`** (`crates/rillml-inspect`): a small CLI binary (not a
  runtime dependency) that reads `Snapshot<T>` JSON files and reports
  schema version, model summary, and validation status. Subcommands:
  `version`, `view-snapshot`, `summary`, `validate`. The CLI is not named
  `rill`, consistent with the project's naming policy.
- **`rill-ml-wasm`** (`crates/rill-ml-wasm`): WebAssembly bindings
  (`wasm32-unknown-unknown` target) exposing a core subset of RillML to the
  browser: `WasmMean`, `WasmVariance`, `WasmEWMean`, `WasmStandardScaler`,
  `WasmLinearRegression`, `WasmLogisticRegression`, `WasmRegressionPipeline`,
  `WasmClassificationPipeline`, and `WasmSnapshot`. Uses `getrandom` with the
  `js` feature; no system threads, no filesystem.
- **`rill-ml-python`** (`crates/rill-ml-python`): Python bindings via PyO3 +
  Maturin. Exposes `Mean`, `Variance`, `EWMean`, `StandardScaler`,
  `LinearRegression`, `LogisticRegression`, `RegressionPipeline`,
  `ClassificationPipeline`, and `Snapshot` to Python with River-style
  `predict_one` / `learn_one` method names. Distributed to PyPI as
  `rill-ml-python`.

### Compatibility notes

- The new crates are additive: `rill-ml`'s public API is unchanged.
- All new crates live under `crates/` and are workspace members, but none are
  dependencies of `rill-ml`. Default builds of `rill-ml` do not pull in
  `tokio`, `arrow`, `polars`, `wasm-bindgen`, or `pyo3`.
- WASM and Python bindings cover the **core subset** (statistics, scalers,
  linear/logistic regression, pipelines, snapshots). FTRL, Naive Bayes, drift
  detectors, bandits, and diagnostics will be added in later minor releases.
- The `rillml-inspect` CLI is a binary crate, not a library. It does not affect
  downstream applications.
- CI gains two new jobs: `wasm-build` (cargo check on `wasm32-unknown-unknown`)
  and `python-build` (maturin develop + pytest on Linux).

## [0.5.2] - 2026-07-14

### Fixed

- The distributed runtime now installs a business-neutral linear-regression
  invoke handler instead of returning `noInvokeHandler` for every request.
- Runtime invocation is restricted to capabilities declared by the signed
  model-pack manifest.
- Release retries preserve existing immutable version assets and can continue
  repairing the mutable stable-index pointer; crate publication also waits for
  bounded crates.io dependency-index propagation.

## [0.5.1] - 2026-07-14

### Changed — Breaking: protocol decoupled from battery business

RillML runtime protocol and loader no longer hardcode battery prediction
business types. This enforces the Roadmap principle (L967-983) that Mira
business names must not leak into the core library.

- `rill-runtime-protocol`: `BatteryModelConfig`, `BatteryPredict`,
  `BatteryPredictionOutput`, `BatterySampleInput`, `BatteryPredictionInput`,
  `BATTERY_USAGE_CAPABILITY`, `PredictionSource` removed.
  `RuntimeRequest` now has a generic `Invoke { capability, input: Value }`
  variant; `RuntimeResponse` has `Result { output: Value }`.
- `rill-runtime`: `battery.rs` module removed. `load_model_pack` no longer
  requires `batteryUsage` capability; `model.json` loaded as generic JSON.
  `RuntimeEngine` accepts an injectable `InvokeHandler` trait.
- `rill-pack` CLI: `create` command accepts any JSON model file.
- Example model renamed `mira.battery.default` → `rillml.example.default`.
- Publisher key id `mira-plugins-2026-001` → `rillml-examples-2026-001`.
- Crates published to crates.io: `rill-ml`, `rill-runtime-protocol`,
  `rill-runtime`.

### Migration

Hosts implementing battery prediction (e.g. Mira) must now define their own
config types and invoke handlers. See section 4 of the decouple plan for
mira-mouse migration steps.

## [0.5.0] - 2026-07-13

### Added — Online decision-making (bandits)

v0.5 introduces bounded-memory multi-armed bandit and contextual bandit
algorithms. Bandits learn to select the best action (arm) from a fixed set by
balancing exploration and exploitation, and are independent from the supervised
learning models in `models`.

- **`Bandit` trait** (`src/bandit/mod.rs`): unified trait for non-contextual
  bandits. `select` is side-effect free; state updates happen only in `update`.
  Includes `ArmStats` for per-arm diagnostics.
- **`ContextualBandit` trait** (`src/bandit/mod.rs`): trait for contextual
  bandits that select an arm based on a context (feature) vector.
- **`EpsilonGreedy`** (`src/bandit/epsilon_greedy.rs`): fixed and
  exponentially-decaying epsilon exploration. O(arm_count) select, O(1) update.
- **`Ucb1`** (`src/bandit/ucb1.rs`): Upper Confidence Bound 1 with
  configurable exploration constant and normalized `[0, 1]` rewards. The
  default constant matches the documented classic UCB1 formula. Unpulled arms
  are prioritized. O(arm_count) select, O(1) update.
- **`ThompsonSampling`** (`src/bandit/thompson.rs`): Beta-distribution
  Thompson Sampling for Bernoulli rewards. Includes internal Marsaglia-Tsang
  Gamma sampling and Box-Muller Normal generation — no external statistics
  crate required. O(arm_count) select, O(1) update.
- **`LinUcb`** (`src/bandit/linucb.rs`): contextual bandit with per-arm linear
  ridge-regression models. Matrix inversion via Gauss-Jordan elimination with
  partial pivoting. O(arm_count * d³) select, O(d²) update,
  O(arm_count * d²) space.
- **New error variants** (`src/error.rs`): `InvalidArmCount`, `InvalidEpsilon`,
  `InvalidArm`, `InvalidReward`, `InvalidFeatureCount`, `InvalidState`.
- **New dependency**: `rand = "0.8"` added as a required dependency (bandits
  fundamentally require randomness for exploration).

### Example

- `examples/bandit_demo.rs`: demonstrates non-contextual bandits comparison
  (EpsilonGreedy vs UCB1 vs ThompsonSampling), LinUCB contextual selection,
  and a safe fallback strategy for cold-start. Run with
  `cargo run --example bandit_demo`.

### Integration tests

- `tests/bandit_learning.rs`: tests for all four bandit algorithms — learning
  convergence (finding the best arm), reset behavior, and error handling.
- `tests/serialization.rs`: 4 new serialization round-trip tests for all
  bandit types (EpsilonGreedy, Ucb1, ThompsonSampling, LinUcb).

### Reliability and release hardening

- Core statistics, regression/classification metrics, SGD, AdaGrad,
  `StandardScaler`, sparse-feature merging, and feature hashing now reject
  arithmetic overflow before committing state.
- SGD and AdaGrad steps are failure-atomic; pipelines expose
  `learn_transactional()` for all-or-nothing transformer/model updates when the
  clone cost is acceptable.
- `StandardScaler`, `SparseFeatures`, and `FeatureHasher` now reject malformed
  persisted state during deserialization, matching the bandit validation model.
- `Snapshot<T>::into_model_with_validation()` adds an application validation
  hook before restored state is activated.
- Added `RELIABILITY.md` with production activation, rollback, observability,
  and release-gate guidance.
- Release packaging no longer skips crate verification or ignores publish
  dry-run failures. CI jobs now have explicit timeouts and concurrency control.
- Added scheduled and dependency-change RustSec advisory audits.

### Compatibility notes

- The bandit module is additive: existing v0.1/v0.2/v0.3/v0.4 APIs are
  unchanged.
- All bandit types implement `Debug`, `Clone`, and (with the `serde` feature)
  `Serialize` / validated `Deserialize`; malformed persisted state is rejected.
- `rand` is now a required dependency (was previously dev-only). This is
  necessary because bandit exploration requires randomness.
- Bandits are independent from supervised learning models. The caller is
  responsible for defining what "reward" means (business layer).
- Beta distribution sampling is implemented internally (Marsaglia-Tsang Gamma
  method); no external statistics crate is required.
- Matrix operations in LinUCB use `Vec<Vec<f64>>` internally; no
  `nalgebra`/`ndarray` dependency is introduced.

## [0.4.0] - 2026-07-13

### Added — Drift detection and adaptation

v0.4 introduces bounded-memory drift detection algorithms, a decoupled
action/strategy layer, decay-aware learning utilities, and a DriftAwareModel
wrapper. This addresses real-world pattern changes: battery aging, user habit
shifts, firmware updates, sensor drift, and service load pattern changes.

- **`DriftDetector` trait** (`src/drift/detector.rs`): unified trait for
  online drift detectors with `DriftLevel` (None / Warning / Drift).
- **`PageHinkley`** (`src/drift/page_hinkley.rs`): cumulative sum test for
  sustained mean shifts. O(1) memory. Detects average-value changes on target
  or prediction-error streams.
- **`Adwin`** (`src/drift/adwin.rs`): Adaptive Windowing detector (Bifet &
  Gavaldà 2007) with warning and drift states. Bucket-compressed windows keep
  memory bounded.
- **`Kswin`** (`src/drift/kswin.rs`): Kolmogorov-Smirnov Windowing detector
  with self-implemented KS CDF (Marsaglia-Tsang-Wang algorithm). Detects
  distribution shape changes, not just mean shifts. O(2 * window_size) memory.
- **`DriftAction` / `DriftStrategy`** (`src/drift/action.rs`,
  `src/drift/strategy.rs`): decoupled action enum
  (NotifyOnly / ReduceConfidence / ResetModel / ResetPreprocessor /
  ReplaceWithBaseline / IncreaseAdaptationRate) and strategy trait. Detectors
  only report drift; strategies decide the response.
- **Decay learning** (`src/drift/decay.rs`): `TimeDecayedMean` (exponential
  decay statistics), `LearningRateScheduler` (dynamic learning rate based on
  drift state), `FixedWindowBuffer` (fixed-window training with recent-data
  priority).
- **`DriftAwareModel<M, D, A>`** (`src/drift/aware_model.rs`): generic wrapper
  that feeds prediction errors to a drift detector and applies the strategy's
  action. Does not auto-reset the model by default; reset only occurs when the
  strategy explicitly returns `ResetModel` or `ResetPreprocessor`.
- **New traits** (`src/traits.rs`): `DriftDetector`, `DriftStrategy`.

### Example

- `examples/drift_demo.rs`: demonstrates Page-Hinkley, ADWIN, and KSWIN on
  synthetic drift scenarios (mean shift, variance change), and shows
  DriftAwareModel automatically resetting a LinearRegression when drift is
  detected. Run with `cargo run --example drift_demo`.

### Integration tests

- `tests/drift_detection.rs`: tests for all three detectors on synthetic drift
  data, DriftAwareModel behavior (event logging, reset action, no auto-reset),
  and decay learning utilities.
- `tests/serialization.rs`: new serialization round-trip tests for all v0.4
  stateful types.

### Compatibility notes

- The drift module is additive: existing v0.1/v0.2/v0.3 APIs are unchanged.
- All drift types implement `Debug`, `Clone`, and (with the `serde` feature)
  `Serialize` / `Deserialize`.
- DriftAwareModel uses generics (`<M, D, A>`) rather than trait objects,
  consistent with the project's concrete-type philosophy.
- KS test p-value is computed via the Marsaglia-Tsang-Wang algorithm; no
  external statistics crate is required.

## [0.3.0] - 2026-07-13

### Added — Sparse features and high-dimensional data

v0.3 introduces sparse feature representation, feature hashing, categorical
encoding, missing value handling, FTRL-Proximal, and online Naive Bayes.

- **`SparseFeatures`** (`src/sparse/mod.rs`): sorted `(FeatureId, f64)` pairs
  with binary search lookup. `FeatureId = u64`. 12 tests.
- **`FeatureHasher`** (`src/feature_hasher.rs`): deterministic hashing from
  string feature names to `FeatureId` buckets with optional signed hashing.
  11 tests.
- **Categorical encoders** (`src/preprocessing/`):
  - `OneHotEncoder`: string categories → one-hot vectors. 9 tests.
  - `OrdinalEncoder`: string categories → integer indices. 7 tests.
  - `FrequencyEncoder`: string categories → observed frequency. 7 tests.
  - `MissingIndicator`: NaN-aware transformer, doubles output dimension. 6 tests.
- **Missing value imputers** (`src/preprocessing/`):
  - `ConstantImputer`: replaces NaN with a fixed value. 6 tests.
  - `MeanImputer`: replaces NaN with per-feature running mean (Welford). 8 tests.
  - `ForwardFill`: replaces NaN with the last seen valid value. 8 tests.
- **New traits** (`src/traits.rs`): `SparseRegressor` and `SparseClassifier`
  accepting `&SparseFeatures`.
- **FTRL-Proximal** (`src/models/ftrl.rs`): `FtrlRegressor` (squared loss) and
  `FtrlClassifier` (log loss) with L1 regularization producing sparse weights.
  Dynamic feature growth via `BTreeMap<FeatureId, FtrlParam>`. 30 tests.
- **Naive Bayes** (`src/models/naive_bayes.rs`): `GaussianNaiveBayes` (Welford
  variance), `BernoulliNaiveBayes` (binary features), `MultinomialNaiveBayes`
  (count features). All implement `OnlineBinaryClassifier`. 35 tests.

### Example

- `examples/sparse_classification.rs`: click prediction demo comparing
  `FtrlClassifier` (sparse) vs `LogisticRegression` (hashed) vs
  `GaussianNaiveBayes` (hashed). Demonstrates FTRL sparse weights (87.5%
  sparsity). Run with `cargo run --example sparse_classification`.

### Integration tests

- `tests/sparse_features.rs`: 8 tests for SparseFeatures and FeatureHasher.
- `tests/ftrl_learning.rs`: 8 tests for FTRL convergence and serialization.
- `tests/naive_bayes.rs`: 8 tests for Naive Bayes classification.
- `tests/serialization.rs`: 14 new serialization round-trip tests for all
  v0.3 stateful types.

### Compatibility notes

- The new `SparseRegressor` / `SparseClassifier` traits are additive: existing
  v0.1/v0.2 APIs are unchanged.
- All new types implement `Debug`, `Clone`, and (with the `serde` feature)
  `Serialize` / `Deserialize`.
- FTRL uses `BTreeMap` for deterministic iteration order and stable
  serialization.
- Imputers accept NaN inputs (they do not call `validate_features`); they
  validate dimension only.

## [0.2.0] - 2026-07-13

### Added — Diagnostics module (`src/diagnostics/`)

v0.2 introduces a bounded-memory diagnostics layer that sits on top of the
core model traits without polluting them. All diagnostic types are `O(1)` or
`O(window_size)` in memory and never store raw samples.

- **`TrainingSummary`** (`training_summary.rs`): tracks `total_samples`,
  `rejected_samples`, recent error (exponentially weighted), best error,
  baseline error, model switches, resets, and load failures. 12 tests.
- **`WarmupTracker`** (`warmup.rs`): lifecycle state machine
  (`NoData` → `WarmingUp` → `Usable` → `Stable` / `Degraded`) driven by
  sample count and error-vs-baseline comparison. 16 tests.
- **`BaselineComparator`** (`baseline_comparator.rs`): compares multiple
  models by rolling MAE and tracks the current best with `SwitchReason`
  (`LowerError` / `Tie` / `InsufficientData`). Does not store the models.
  16 tests.
- **`OnlineModelSelector`** (`model_selector.rs`): wraps
  `BaselineComparator` with a cooling period and minimum-samples gate
  before allowing a switch. 15 tests.
- **`ResidualInterval`** / **`PredictionInterval`**
  (`prediction_interval.rs`): residual-based prediction intervals
  (`prediction ± k × recent_error`) with configurable quantile. 14 tests.
- **`ModelHealthReport`** (`model_health.rs`): inspects model parameters
  for `NaN` / `Infinity`, reports weight range and state size. 10 tests.
- **`PredictionReporter`** (`prediction_report.rs`): integrates
  `ResidualInterval`, `WarmupTracker`, and `TrainingSummary` into a single
  `PredictionReport` with a `Confidence` level (`Low` / `Medium` / `High`).
  11 tests.

### Example

- `examples/diagnostics_demo.rs`: demonstrates `TrainingSummary`,
  `PredictionReporter`, `OnlineModelSelector` (comparing `MeanRegressor`
  vs `LinearRegression`), and `ModelHealthReport` on a synthetic linear
  stream. Run with `cargo run --example diagnostics_demo`.

### Compatibility notes

- The diagnostics module is additive: existing v0.1 APIs are unchanged.
- All diagnostic types implement `Debug`, `Clone`, and (with the `serde`
  feature) `Serialize` / `Deserialize`.

## [0.1.0] - 2026-07-12

The first usable release of RillML. RillML is an online (single-pass, bounded
memory) machine learning library for Rust, inspired by the workflow popularized
by River but implemented independently.

### Added

- **Online statistics** (`src/stats/`): `Count`, `Sum`, `Mean`, `Variance`
  (Welford, population and sample), `StandardDeviation`, `Min`, `Max`,
  `ExponentiallyWeightedMean`, `RollingMean`, `RollingVariance`. Non-rolling
  statistics use `O(1)` memory; rolling statistics use `O(window_size)`.
- **Preprocessing** (`src/preprocessing/`): `StandardScaler`, `MinMaxScaler`,
  `Clipper`. Transformers never observe the target label.
- **Models** (`src/models/`): `LinearRegression` (SGD/AdaGrad, L2,
  SquaredError/HuberLoss), `LogisticRegression` (BinaryLogLoss), and baseline
  regressors `LastValueRegressor`, `MeanRegressor`,
  `ExponentiallyWeightedMeanRegressor`.
- **Optimizers** (`src/optim/`): concrete `Optimizer` enum with `Sgd` and
  `AdaGrad` variants. No trait objects.
- **Losses** (`src/loss/`): concrete `RegressionLoss` enum
  (`SquaredError`, `Huber(HuberLoss)`) and `BinaryLogLoss` with a numerically
  stable `sigmoid`.
- **Metrics** (`src/metrics/`): regression (`Mae`, `Mse`, `Rmse`, `R2`,
  `RollingMae`, `RollingMse`) and classification (`Accuracy`, `Precision`,
  `Recall`, `F1Score`, `LogLoss`, `RollingAccuracy`).
- **Pipelines** (`src/pipeline.rs`): `RegressionPipeline<T, M>` and
  `ClassificationPipeline<T, M>` with a fixed transformer + model layout and the
  `predict → metric.update → learn` contract.
- **Progressive evaluation** (`src/evaluate/progressive.rs`):
  `progressive_regress` and `progressive_classify` enforcing the
  predict-before-learn order with side-effect-free predictions.
- **Persistence** (`src/persistence.rs`): optional `serde` feature with a
  versioned `Snapshot<T>` envelope (`format_version`, `model`).
- **Examples** (`examples/`): `online_regression`, `sensor_stream`,
  `online_classification`, `progressive_validation`. All use fixed seeds for
  reproducibility.
- **Benchmarks** (`benches/`): `online_stats` and `online_models` via
  `criterion`.
- **Integration tests** (`tests/`): `stats_reference`, `regression_learning`,
  `classification_learning`, `pipeline_behavior`, `progressive_order`,
  `serialization`.
- **Documentation**: Chinese `README.md` (primary) and English `README.en.md`,
  `LICENSE-MIT`, `CHANGELOG.md`, `CONTRIBUTING.md`,
  `CODE_OF_CONDUCT.md`, `SECURITY.md`, `THIRD_PARTY_NOTICES.md`.
- **CI**: GitHub Actions for `fmt`, `clippy`, `test`, `doc`, and release
  packaging checks on Linux, macOS, and Windows with the MSRV 1.85.

### Non-goals (explicitly out of scope for 0.1)

- Drift detection (Page-Hinkley, ADWIN, KSWIN).
- FTRL-Proximal and sparse `FeatureId` inputs.
- Hoeffding Tree, online ensembles, Naive Bayes.
- Multi-armed bandits and contextual bandits.
- Python bindings, WebAssembly targets, `no_std` subset.
- Dynamic, arbitrarily-composed pipelines.
- Claiming `no_std` support.

### Compatibility notes

- The `Snapshot<T>` format is versioned but **not** guaranteed to be stable
  across 0.x releases. Restore through `into_model()` so `format_version` is
  checked before activating state.
- Random examples and tests use fixed seeds (`ChaCha8Rng::seed_from_u64`) so
  outputs are reproducible.
- Only `f64` is supported. Dense `&[f64]` feature slices only; no
  `HashMap<String, f64>`.

[Unreleased]: https://github.com/hello-yunshu/rill-ml/compare/v1.3.0...HEAD
[1.3.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v1.3.0
[1.2.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v1.2.0
[1.2.0-rc.3]: https://github.com/hello-yunshu/rill-ml/releases/tag/v1.2.0-rc.3
[1.2.0-rc.2]: https://github.com/hello-yunshu/rill-ml/releases/tag/v1.2.0-rc.2
[1.2.0-rc.1]: https://github.com/hello-yunshu/rill-ml/releases/tag/v1.2.0-rc.1
[1.1.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v1.1.0
[1.0.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v1.0.0
[1.0.0-rc.6]: https://github.com/hello-yunshu/rill-ml/releases/tag/v1.0.0-rc.6
[1.0.0-rc.5]: https://github.com/hello-yunshu/rill-ml/releases/tag/v1.0.0-rc.5
[1.0.0-rc.4]: https://github.com/hello-yunshu/rill-ml/releases/tag/v1.0.0-rc.4
[1.0.0-rc.3]: https://github.com/hello-yunshu/rill-ml/releases/tag/v1.0.0-rc.3
[1.0.0-rc.2]: https://github.com/hello-yunshu/rill-ml/releases/tag/v1.0.0-rc.2
[1.0.0-rc.1]: https://github.com/hello-yunshu/rill-ml/releases/tag/v1.0.0-rc.1
[0.13.0]: https://github.com/hello-yunshu/rill-ml/compare/v0.12.0...v0.13.0
[0.12.0]: https://github.com/hello-yunshu/rill-ml/compare/v0.10.0...v0.12.0
[0.10.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.10.0
[0.9.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.9.0
[0.8.1]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.8.1
[0.8.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.8.0
[0.7.2]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.7.2
[0.7.1]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.7.1
[0.7.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.7.0
[0.6.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.6.0
[0.5.2]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.5.2
[0.5.1]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.5.1
[0.5.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.5.0
[0.4.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.4.0
[0.3.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.3.0
[0.2.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.2.0
[0.1.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.1.0
