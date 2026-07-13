# RillML

Lightweight, serializable online machine learning for Rust applications and streaming data.

RillML provides incremental learning primitives that can be embedded directly in native Rust applications: online statistics, preprocessors, linear/logistic regression, evaluation metrics, pipelines, progressive evaluation, and optional serde-based state persistence.

> RillML is inspired by the online-learning workflow popularized by [River](https://riverml.xyz/). It is an independent Rust project and is not affiliated with or endorsed by River. It does not currently aim for API or model compatibility.

---

## Why online learning?

Traditional machine learning follows a batch workflow: collect data, train offline, deploy a fixed model, and periodically retrain. This works well when data is abundant, static, and centrally available.

Online learning takes a different approach: process one sample at a time, predict before learning, and adapt continuously. This is well-suited for:

- **Streaming data** where you cannot store all history.
- **Edge devices** with limited memory and no Python runtime.
- **Continuously changing environments** where a fixed model goes stale.
- **Privacy-sensitive scenarios** where data should not leave the device.
- **Real-time systems** that need predictions before the next sample arrives.

RillML implements this workflow in pure, safe Rust with bounded memory.

---

## Suitable scenarios

- Online regression for IoT telemetry, resource usage, or sensor readings.
- Sensor anomaly detection with rolling statistics.
- Real-time click or event classification.
- Network latency prediction with concept drift.
- Any Rust application that needs a lightweight, always-on learning component.

## Non-suitable scenarios

- Large-scale offline model training (use Linfa, SmartCore, or Python instead).
- Deep learning (use Burn, candle, or tch-rs).
- Distributed training across multiple machines.
- Scenarios requiring GPU acceleration.
- Research and rapid algorithm experimentation (Python is better suited).

Python is more appropriate for research, data analysis, and fast algorithm experimentation. RillML focuses on Rust-native embedding and continuous operation. Rust does not make the same algorithm inherently more accurate; the value comes from engineering deployment, state management, and local execution.

---

## Installation

Add RillML to your `Cargo.toml`:

```toml
[dependencies]
rill-ml = "0.1"
```

For serialization support, enable the `serde` feature:

```toml
[dependencies]
rill-ml = { version = "0.1", features = ["serde"] }
```

**Requirements:** Rust 1.85+ (Edition 2024), no nightly needed.

---

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

---

## Progressive evaluation

The core contract of online learning is: **predict before you learn**. RillML's `evaluate` module enforces this order:

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

---

## Regression example

See [`examples/online_regression.rs`](examples/online_regression.rs) for a full online regression demo that:
- Compares `MeanRegressor`, `EWMeanRegressor`, and `LinearRegression`.
- Uses `StandardScaler` for feature normalization.
- Demonstrates `Snapshot` serialization round-trip.

```sh
cargo run --example online_regression --features serde
```

---

## Classification example

See [`examples/online_classification.rs`](examples/online_classification.rs) for online binary classification with `LogisticRegression`:

```sh
cargo run --example online_classification
```

---

## Diagnostics example

See [`examples/diagnostics_demo.rs`](examples/diagnostics_demo.rs) for the v0.2 diagnostics module:

- Uses `TrainingSummary` to track training statistics.
- Uses `PredictionReporter` to generate reports with confidence levels and prediction intervals.
- Uses `OnlineModelSelector` to compare `MeanRegressor` vs `LinearRegression` and auto-select the best model.
- Uses `ModelHealthReport` to detect NaN/Infinity in model parameters.

```sh
cargo run --example diagnostics_demo
```

---

## Sparse features example

See [`examples/sparse_classification.rs`](examples/sparse_classification.rs) for high-dimensional sparse classification:

- Uses `SparseFeatures` to represent sparse feature vectors.
- Uses `FeatureHasher` to hash string feature names into `FeatureId` buckets.
- Compares `FtrlClassifier` (sparse input) vs `LogisticRegression` (hashed dense input) vs `GaussianNaiveBayes`.
- Demonstrates sparse weights produced by FTRL's L1 regularization.

```sh
cargo run --example sparse_classification
```

`SparseFeatures` uses a sorted `Vec<(u64, f64)>` instead of `HashMap`, supporting binary search lookup and deterministic serialization. `FtrlRegressor` / `FtrlClassifier` achieve dynamic feature growth via `BTreeMap<FeatureId, FtrlParam>`, without needing to know all feature IDs in advance.

---

## Drift detection example

See [`examples/drift_demo.rs`](examples/drift_demo.rs), demonstrating the v0.4 drift detection module:

- Uses `PageHinkley` to detect mean shifts.
- Uses `Adwin` to detect adaptive-window distribution changes.
- Uses `Kswin` to detect distribution shape changes.
- Demonstrates `DriftAwareModel` automatically resetting `LinearRegression` when drift is detected.

```sh
cargo run --example drift_demo
```

---

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

`Snapshot<T>` wraps model state with a format version for forward compatibility.

---

## Baselines

RillML provides three simple baseline regressors:

- **`MeanRegressor`** — predicts the running mean of all targets seen.
- **`ExponentiallyWeightedMeanRegressor`** — weights recent targets more heavily.
- **`LastValueRegressor`** — predicts the last seen target.

Always compare your model against baselines using progressive evaluation. Only trust a complex model if it consistently beats baselines.

---

## Current scope (v0.4)

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

Memory bounds:
- Non-rolling statistics: O(1)
- Linear models: O(d) where d = feature count
- Rolling statistics: O(window_size)
- Diagnostics: O(1) or O(window_size), never stores raw samples
- Sparse models (FTRL): O(k) where k = seen feature count (not total feature space)
- Categorical encoders: O(c) where c = seen category count
- Drift detectors: O(1) (PageHinkley) or O(window_size) (Adwin/Kswin)
- DriftAwareModel: O(max_events) event log + model + detector

---

## Roadmap

RillML follows a real-need-driven roadmap. See [RillML_Roadmap.md](RillML_Roadmap(1).md) for the full plan.

- **v0.1** — Basic closed loop: predict, evaluate, learn, save, restore.
- **v0.2** — Reliability and diagnostics: prediction reports, cold-start, baseline comparison.
- **v0.3** — Sparse features and high-dimensional data: FeatureHasher, FTRL, Naive Bayes.
- **v0.4** — Drift detection: Page-Hinkley, ADWIN, KSWIN, adaptive learning. *(current)*
- **v0.5** — Online decision-making: multi-armed bandits, contextual bandits.
- **v0.6** — Platform and ecosystem: WASM, Python bindings, Tokio Stream adapters.
- **v1.0** — Stable API and state format.

---

## Correctness and validation

RillML is validated through multiple layers:

- **Unit tests** for every module (451 tests).
- **Integration tests** comparing online algorithms against batch reference formulas (112 tests).
- **Doctests** for all public APIs (31 tests).
- **Serialization round-trip tests** for all stateful types.
- **Property-based tests** with `proptest`.
- **Deterministic tests** with fixed seeds (`rand_chacha`).
- **Clippy** with `-D warnings` in CI.
- **rustfmt** enforced.
- **Example run verification**: all examples are actually run and verified.

Numerical stability:
- Welford's algorithm for variance.
- Numerically stable sigmoid.
- Epsilon-guarded scaling to avoid division by zero.
- No panics in public APIs; all errors are returned as `Result<_, RillError>`.

---

## Relationship to River

RillML is inspired by the online-learning workflow popularized by [River](https://riverml.xyz/). It is an independent Rust project and is not affiliated with or endorsed by River. It does not currently aim for API or model compatibility.

River remains an excellent choice for Python-based online learning research and experimentation.

---

## Relationship to Linfa, SmartCore, and Burn

- **[Linfa](https://github.com/rust-ml/linfa)** — A comprehensive Rust ML toolkit inspired by scikit-learn. Linfa focuses on batch learning. RillML focuses on online/incremental learning with bounded memory.
- **[SmartCore](https://smartcorelib.org/)** — A fast Rust ML library with broad algorithm coverage. SmartCore is primarily batch-oriented. RillML is designed for streaming and edge deployment.
- **[Burn](https://burn-rs.github.io/)** — A deep learning framework in Rust. Burn targets neural networks and GPU computation. RillML targets lightweight online models that can run anywhere.

These projects are complementary, not competitive. RillML does not aim to replace them.

---

## Naming note

This project is named **RillML**. It is not affiliated with, endorsed by, or related to [Rill Data](https://www.rilldata.com/) or any product named "Rill". RillML does not provide a CLI tool named `rill`.

---

## License

Licensed under the MIT License ([LICENSE-MIT](LICENSE-MIT)).

---

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) before submitting a pull request.

RillML follows a "real-need-driven" development principle: every new feature should solve a real problem in a real Rust application, not just replicate what exists in other frameworks. See the [Roadmap](RillML_Roadmap(1).md) for prioritized directions.
