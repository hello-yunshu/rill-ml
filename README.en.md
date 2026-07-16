<p align="center">
  <img src="logo.png" alt="RillML" width="480">
</p>

<p align="center">
  Lightweight online machine learning for Rust applications, edge devices, and continuously changing data streams
</p>

<p align="center">
  <a href="https://github.com/hello-yunshu/rill-ml/actions/workflows/pipeline.yml"><img src="https://github.com/hello-yunshu/rill-ml/actions/workflows/pipeline.yml/badge.svg" alt="CI / Release"></a>
  <a href="https://crates.io/crates/rill-ml"><img src="https://img.shields.io/crates/v/rill-ml.svg" alt="crates.io"></a>
  <a href="https://docs.rs/rill-ml"><img src="https://docs.rs/rill-ml/badge.svg" alt="docs.rs"></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/crates/l/rill-ml.svg" alt="License: MIT"></a>
  <img src="https://img.shields.io/badge/rust-1.94%2B-orange.svg" alt="Rust 1.94+">
</p>

<p align="center">
  <a href="README.md">中文</a> &middot; <a href="CHANGELOG.md">Changelog</a> &middot; <a href="ROADMAP.md">Roadmap</a> &middot; <a href="https://docs.rs/rill-ml">API Docs</a>
</p>

---

RillML provides incremental learning primitives that can be embedded directly in native Rust applications: online statistics, preprocessors, linear/logistic regression, evaluation metrics, pipelines, progressive evaluation, and optional serde-based state persistence.

The workspace also includes a separately distributable `rill-runtime`, a stable IPC contract, signed `.rillpack` model packages, and signed `.rillhandler` WASM handler packages. As of v0.7, the runtime loads signature-verified WASM handlers in a sandbox; updating a handler no longer requires recompiling the runtime binary. Hosts can compile only the protocol crate and update the runtime, models, and handlers independently from the main application. Official macOS Runtime releases support Apple Silicon (ARM64) only; no Intel build is provided. See [`RUNTIME.md`](RUNTIME.md) for the product and release boundary.

> RillML is inspired by the online-learning workflow popularized by [River](https://riverml.xyz/). It is an independent Rust project and is not affiliated with or endorsed by River. It does not currently aim for API or model compatibility.

## Why online learning?

Traditional machine learning follows a batch workflow: collect data, train offline, deploy a fixed model, and periodically retrain. This works well when data is abundant, static, and centrally available.

Online learning takes a different approach: **process one sample at a time, predict before learning, and adapt continuously**. This is well-suited for:

- **Streaming data** — you cannot store all history.
- **Edge devices** — limited memory, no Python runtime.
- **Continuously changing environments** — a fixed model goes stale.
- **Privacy-sensitive scenarios** — data should not leave the device.
- **Real-time systems** — predictions needed before the next sample arrives.

RillML implements this workflow in pure, safe Rust with bounded memory.

## Suitable scenarios

- Online regression for IoT telemetry, resource usage, or sensor readings.
- Sensor anomaly detection with rolling statistics.
- Real-time click or event classification.
- Network latency prediction with concept drift.
- Any Rust application that needs a lightweight, always-on learning component.

**Non-suitable scenarios:** Large-scale offline training (use Linfa/SmartCore/Python), deep learning (use Burn/candle/tch-rs), distributed training, GPU acceleration, research experimentation (Python is better suited). Rust does not make the same algorithm inherently more accurate; the value comes from engineering deployment, state management, and local execution.

## Installation

```toml
[dependencies]
rill-ml = "0.7"
```

For serialization support, enable the `serde` feature:

```toml
[dependencies]
rill-ml = { version = "0.7", features = ["serde"] }
```

**Requirements:** Rust 1.94+ (Edition 2024), no nightly needed.

## Quick start

```rust
use rill_ml::{
    metrics::Mae,
    models::{LinearRegression, LinearRegressionConfig},
    optim::{Optimizer, SgdConfig},
    pipeline::RegressionPipeline,
    preprocessing::StandardScaler,
    Metric, OnlineRegressor,
};

let feature_count = 2;
let scaler = StandardScaler::new(feature_count).unwrap();
let optimizer = Optimizer::sgd(
    feature_count,
    SgdConfig { learning_rate: 0.05, l2: 0.0 },
).unwrap();
let regression = LinearRegression::new(
    feature_count,
    LinearRegressionConfig { optimizer, loss: Default::default() },
).unwrap();
let mut model = RegressionPipeline::new(scaler, regression).unwrap();
let mut mae = Mae::default();

let samples = [
    ([0.1, 0.2], 0.5),
    ([0.3, 0.8], 1.4),
    ([0.6, 0.4], 1.1),
];
for (features, target) in samples {
    let prediction = model.predict(&features).unwrap();
    mae.update(target, prediction).unwrap();
    model.learn(&features, target).unwrap();
}
```

## Progressive evaluation

The core contract of online learning is: **predict before you learn**. The `evaluate` module enforces this order:

```text
predict  →  metric.update  →  learn
```

This ensures metrics reflect the model's ability to generalize to *unseen* data, not memorized samples.

```rust
use rill_ml::evaluate::{evaluate_regression, RegressionSample};
use rill_ml::metrics::Mae;
use rill_ml::models::{BaselineConfig, MeanRegressor};
use rill_ml::OnlineRegressor;

let mut model = MeanRegressor::new(BaselineConfig::default()).unwrap();
let mut mae = Mae::default();

let samples = vec![
    RegressionSample { features: vec![], target: 10.0 },
    RegressionSample { features: vec![], target: 20.0 },
    RegressionSample { features: vec![], target: 30.0 },
];

let final_mae = evaluate_regression(&mut model, &mut mae, samples).unwrap();
```

## Examples

| Example | Description | Command |
|---|---|---|
| [online_regression](examples/online_regression.rs) | Compare Mean/EWMean/LinearRegression, StandardScaler, Snapshot serialization | `cargo run --example online_regression --features serde` |
| [online_classification](examples/online_classification.rs) | Online binary classification with LogisticRegression | `cargo run --example online_classification` |
| [diagnostics_demo](examples/diagnostics_demo.rs) | TrainingSummary, PredictionReporter, OnlineModelSelector, ModelHealthReport | `cargo run --example diagnostics_demo` |
| [sparse_classification](examples/sparse_classification.rs) | SparseFeatures, FeatureHasher, FTRL, NaiveBayes high-dim sparse classification | `cargo run --example sparse_classification` |
| [drift_demo](examples/drift_demo.rs) | Page-Hinkley, ADWIN, KSWIN drift detection with DriftAwareModel | `cargo run --example drift_demo` |
| [bandit_demo](examples/bandit_demo.rs) | EpsilonGreedy, UCB1, ThompsonSampling, LinUCB online decision-making | `cargo run --example bandit_demo` |
| [sensor_stream](examples/sensor_stream.rs) | Sensor data stream online statistics | `cargo run --example sensor_stream` |
| [progressive_validation](examples/progressive_validation.rs) | Progressive evaluation flow demo | `cargo run --example progressive_validation` |

## Serialization

Enable the `serde` feature to serialize and restore model state:

```rust
use rill_ml::persistence::Snapshot;
use rill_ml::stats::Mean;
use rill_ml::OnlineStatistic;

let mut mean = Mean::new();
mean.update(1.0).unwrap();
mean.update(2.0).unwrap();

let snap = Snapshot::new(mean);
let json = serde_json::to_string(&snap).unwrap();
let restored: Snapshot<Mean> = serde_json::from_str(&json).unwrap();
let m = restored.into_model().unwrap();
assert!((m.value() - 1.5).abs() < 1e-12);
```

`Snapshot<T>` wraps model state with a format version and rejects incompatible versions. For untrusted snapshots or application-specific model constraints, use `into_model_with_validation()` to validate restored state before activation. See [`RELIABILITY.md`](RELIABILITY.md) for the complete production integration and fallback guidance.

## Module overview (v0.7)

| Category | Modules |
|---|---|
| Statistics | Mean, Variance, Std, Count, Sum, Min, Max, EWMean, RollingMean, RollingVariance |
| Preprocessing | StandardScaler, MinMaxScaler, Clipper, OneHotEncoder, OrdinalEncoder, FrequencyEncoder, MissingIndicator, ConstantImputer, MeanImputer, ForwardFill |
| Sparse features | SparseFeatures, FeatureHasher |
| Models | LinearRegression, LogisticRegression, MeanRegressor, EWMeanRegressor, LastValueRegressor, FtrlRegressor, FtrlClassifier, GaussianNaiveBayes, BernoulliNaiveBayes, MultinomialNaiveBayes |
| Optimizers | SGD (with L2), AdaGrad |
| Losses | SquaredError, HuberLoss, BinaryLogLoss |
| Metrics (regression) | MAE, MSE, RMSE, R², RollingMAE, RollingMSE |
| Metrics (classification) | Accuracy, Precision, Recall, F1, LogLoss, RollingAccuracy |
| Pipelines | RegressionPipeline, ClassificationPipeline |
| Evaluation | Progressive evaluation (predict → metric → learn) |
| Persistence | `Snapshot<T>` with versioned envelope (serde feature) |
| Diagnostics | TrainingSummary, WarmupTracker, BaselineComparator, OnlineModelSelector, ResidualInterval, ModelHealthReport, PredictionReporter |
| Drift detection | PageHinkley, Adwin, Kswin, DriftAwareModel, DriftAction, DriftStrategy, TimeDecayedMean, LearningRateScheduler, FixedWindowBuffer |
| Online decision-making | EpsilonGreedy, Ucb1, ThompsonSampling, LinUcb, ArmStats |

**Memory bounds:** Non-rolling statistics O(1); linear models O(d); rolling statistics O(window_size); sparse models (FTRL) O(k), k = seen feature count; drift detectors O(1) or O(window_size); LinUCB O(arm_count × d²).

## Ecosystem and platform extensions (v0.7)

v0.6 adds five independently publishable crates; v0.7 adds `rill-handler-api`. They live under `crates/` and depend on `rill-ml` without changing its public API. The core library does not pull in `tokio`/`arrow`/`polars`/`wasm-bindgen`/`pyo3` by default.

| Crate | Description | Install |
|---|---|---|
| `rill-ml-tokio` | Drives `predict → metric → learn` over a `tokio_stream::Stream` | `cargo add rill-ml-tokio` |
| `rill-ml-arrow` | Convert between Apache Arrow `RecordBatch`/`Float64Array` and `&[f64]` | `cargo add rill-ml-arrow` |
| `rill-ml-polars` | Convert between Polars `DataFrame` and sample pairs; append prediction column | `cargo add rill-ml-polars` |
| `rillml-inspect` | CLI to view `Snapshot` JSON, version, and validation status (not a runtime dependency) | `cargo install rillml-inspect` |
| `rill-ml-wasm` | WebAssembly bindings (`wasm32-unknown-unknown`) for browser-side online learning | `cargo add rill-ml-wasm` |
| `rill-ml-python` | Python bindings (PyO3 + Maturin); PyPI package `rill-ml-python`, `import rill_ml` | `pip install rill-ml-python` |
| `rill-handler-api` | Versioned WIT handler ABI contract (for handler authors) | `cargo add rill-handler-api` |
| `rill-runtime-protocol` | Stable, strict, versioned JSON IPC types | `cargo add rill-runtime-protocol` |
| `rill-runtime` | Standalone executable runtime that loads signed model and handler packs | `cargo install rill-runtime` |

## Roadmap

RillML follows a real-need-driven roadmap. See [`ROADMAP.md`](ROADMAP.md) for the full plan.

- **v0.1** — Basic closed loop: predict, evaluate, learn, save, restore.
- **v0.2** — Reliability and diagnostics: prediction reports, cold-start, baseline comparison.
- **v0.3** — Sparse features and high-dimensional data: FeatureHasher, FTRL, Naive Bayes.
- **v0.4** — Drift detection: Page-Hinkley, ADWIN, KSWIN, adaptive learning.
- **v0.5** — Online decision-making: multi-armed bandits, contextual bandits.
- **v0.6** — Platform and ecosystem: WASM, Python bindings, Tokio Stream adapters.
- **v0.7** — Pluggable WASM handlers: signed `.rillhandler` packs, Wasmtime sandbox, IPC v2. *(current)*
- **v1.0** — Stable API and state format.

## Correctness and validation

RillML is validated through multiple layers:

- **562** unit tests + **130** integration tests + **40** doctests.
- Serialization round-trip tests for all stateful types.
- `proptest` property-based tests with fixed seeds (`rand_chacha`).
- Clippy with `-D warnings` in CI; rustfmt enforced.
- All examples are actually run and verified.

**Numerical stability:** Welford's algorithm for variance; numerically stable sigmoid; epsilon-guarded scaling; no panics in public APIs, all errors returned as `Result<_, RillError>`.

## Related projects

| Project | Focus | Relationship to RillML |
|---|---|---|
| [River](https://riverml.xyz/) | Python online learning | RillML is inspired by its workflow, independently implemented, no compatibility target |
| [Linfa](https://github.com/rust-ml/linfa) | Rust batch learning toolkit | Batch-focused; RillML focuses on online/incremental learning |
| [SmartCore](https://smartcorelib.org/) | Rust ML library | Primarily batch-oriented; RillML targets streaming and edge deployment |
| [Burn](https://burn-rs.github.io/) | Rust deep learning framework | Targets neural networks and GPU; RillML targets lightweight online models |

These projects are complementary, not competitive.

## Naming note

This project is named **RillML**. It is not affiliated with, endorsed by, or related to [Rill Data](https://www.rilldata.com/) or any product named "Rill". RillML does not provide a CLI tool named `rill`.

## License

Licensed under the MIT License ([LICENSE-MIT](LICENSE-MIT)).

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) before submitting a pull request. RillML follows a "real-need-driven" development principle: every new feature should solve a real problem in a real Rust application.
