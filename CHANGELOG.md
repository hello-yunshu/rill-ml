# Changelog

All notable changes to RillML will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
with the Rust-specific convention that 0.x releases may break the public API.

> **Status: 0.x — Experimental but usable.**
> The core math is tested and the predict/learn/persist loop is complete, but
> the public API may still change between minor versions. Do not use RillML for
> safety-critical, medical, financial, or industrial-control decisions without
> independent verification. Always keep a simple baseline and business-rule
> fallback alongside model predictions.

## [Unreleased]

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

[Unreleased]: https://github.com/hello-yunshu/rill-ml/compare/v0.8.1...HEAD
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
