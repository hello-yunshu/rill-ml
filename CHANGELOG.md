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

[Unreleased]: https://github.com/hello-yunshu/rill-ml/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.5.0
[0.4.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.4.0
[0.3.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.3.0
[0.2.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.2.0
[0.1.0]: https://github.com/hello-yunshu/rill-ml/releases/tag/v0.1.0
