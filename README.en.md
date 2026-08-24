<p align="center">
  <img src="logo.png" alt="RillML" width="480">
</p>

<p align="center">
  <strong>RillML (Rill)</strong> is a lightweight adaptive intelligence runtime for native and edge applications.
</p>

<p align="center">
  It combines online machine learning, bounded adaptation, stable runtime protocols, signed model/handler assets, and safe host integration for continuously changing systems.
</p>

<p align="center">
  <a href="https://github.com/hello-yunshu/rill-ml/actions/workflows/pipeline.yml"><img src="https://github.com/hello-yunshu/rill-ml/actions/workflows/pipeline.yml/badge.svg" alt="CI / Release"></a>
  <a href="https://crates.io/crates/rill-ml"><img src="https://img.shields.io/crates/v/rill-ml.svg" alt="crates.io"></a>
  <a href="https://docs.rs/rill-ml"><img src="https://docs.rs/rill-ml/badge.svg" alt="docs.rs"></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/crates/l/rill-ml.svg" alt="License: MIT"></a>
  <img src="https://img.shields.io/badge/rust-1.94%2B-orange.svg" alt="Rust 1.94+">
</p>

<p align="center">
  <a href="README.md">中文</a> &middot; <a href="CHANGELOG.md">Changelog</a> &middot; <a href="ROADMAP.md">Roadmap</a> &middot; <a href="PLATFORM_SUPPORT.md">Platform Support</a> &middot; <a href="https://docs.rs/rill-ml">API Docs</a>
</p>

---

## 1. What is RillML

**RillML** (referred to as **Rill** where the context is unambiguous) is a lightweight adaptive intelligence runtime for native and edge applications.

It is not "just another online machine learning library". It combines the following capabilities into a single independently distributable runtime:

- **Online ML core**: statistics, regression/classification, drift detection, multi-armed bandits and decision primitives, all implemented in pure safe Rust with bounded state;
- **RillML Runtime (`rill-runtime`)**: a standalone local inference runtime that loads signature-verified WASM handlers in a sandbox, so hosts can integrate safely without linking the engine;
- **Signed assets**: `.rillpack` model packs, `.rillhandler` handler packs and the Ed25519-signed release index, so every update is verifiable and rollback-able;
- **Stable protocol boundaries**: `rill-runtime-protocol` provides frozen v1/v2 JSON IPC; `rill-handler-api` provides the frozen v1 WIT ABI;
- **Safe host integration**: models and handlers can never modify host state directly; all system changes belong to the host governance layer.

The Stable `serve` Runtime CLI continues to accept only IPC v1/v2. Preview IPC
v3 is exposed only through the explicit `preview-serve --state PATH`
subprocess surface, whose handshake carries machine-readable
`channel: "preview"`; it does not change `serve` semantics or get enabled
implicitly. The production `serve` command must explicitly
entrypoint. `serve` requires an explicit signed `--handler` or the deprecated
`--builtin-handler linear-regression`; omitting both fails closed rather than
silently falling back.

The workspace also includes a separately distributable `rill-runtime`, a stable IPC contract, signed `.rillpack` model packages, and signed `.rillhandler` WASM handler packages. The runtime loads signature-verified WASM handlers in a sandbox; updating a handler no longer requires recompiling the runtime binary. Hosts can compile only the protocol crate and update the runtime, models, and handlers independently from the main application. Official macOS Runtime releases support Apple Silicon (ARM64) only; no Intel build is provided. The macOS aarch64 asset is unsigned — the project does not configure an Apple Developer ID certificate, and this is a permanent decision that does not block any release. macOS users may need to authorize first launch through standard controls such as Finder right-click → Open or System Settings → Privacy & Security; see [`STABILITY.md`](STABILITY.md) § macOS unsigned policy for details. The 1.0 stability matrix, frozen surface, and Stable/Preview split are documented in [`STABILITY.md`](STABILITY.md); see [`RUNTIME.md`](RUNTIME.md) for the runtime product and release boundary.

> RillML is inspired by the online-learning workflow popularized by [River](https://riverml.xyz/). It is an independent Rust project and is not affiliated with or endorsed by River. It does not currently aim for API or model compatibility.

## 2. Why RillML

### Why online learning?

Traditional machine learning follows a batch workflow: collect data, train offline, deploy a fixed model, and periodically retrain. This works well when data is abundant, static, and centrally available.

Online learning takes a different approach: **process one sample at a time, predict before learning, and adapt continuously**. This is well-suited for:

- **Streaming data** — you cannot store all history.
- **Edge devices** — limited memory, no Python runtime.
- **Continuously changing environments** — a fixed model goes stale.
- **Privacy-sensitive scenarios** — data should not leave the device.
- **Real-time systems** — predictions needed before the next sample arrives.

RillML implements this workflow in pure, safe Rust; fixed-dimension algorithms use bounded state, while dynamic-feature algorithms such as FTRL require `max_features` to be set for bounded state.

### Suitable scenarios

- Online regression for IoT telemetry, resource usage, or sensor readings.
- Sensor anomaly detection with rolling statistics.
- Real-time click or event classification.
- Network latency prediction with concept drift.
- Any Rust application that needs a lightweight, always-on learning component.

**Non-suitable scenarios:** Large-scale offline training (use Linfa/SmartCore/Python), deep learning (use Burn/candle/tch-rs), distributed training, GPU acceleration, research experimentation (Python is better suited). Rust does not make the same algorithm inherently more accurate; the value comes from engineering deployment, state management, and local execution.

### Product boundary

RillML's core value is not "the most algorithms", but:

```text
online adaptation
bounded resource usage
native/edge deployment
stable protocol boundaries
signed artifacts
safe runtime execution
state lifecycle
feedback/outcome loop
drift handling
decision primitives
product integration
```

Relationship to River:

```text
River   = broad online ML algorithms / experimentation ecosystem
RillML  = production-oriented adaptive intelligence runtime for native and edge systems
```

River can be used as a reference, baseline, and offline validation source, but Python is never introduced as a production dependency on target devices.

## 3. Core capabilities

### Module overview

| Category | Modules |
|---|---|
| Statistics | Mean, Variance, Std, Count, Sum, Min, Max, EWMean, RollingMean, RollingVariance, P2Quantile, ClippedMean, bounded RollingMedianMad/modified-z, weighted statistics |
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
| Drift detection | PageHinkley, Adwin, Kswin, portable state V1, DriftConsensus, DriftAwareModel, DriftAction, DriftStrategy |
| Online decision-making | EpsilonGreedy, Ucb1, ThompsonSampling, LinUcb score breakdown, Preview LinUcbFast, DecisionLedger, DecisionReplayHarness |
| Identity and weights | FeatureSchema, ModelDescriptor, WeightedStatistic/Regressor/Classifier |

**Memory bounds:** Non-rolling statistics O(1); linear models O(d); rolling statistics O(window_size); sparse models (FTRL) O(k), k = seen feature count (unbounded by default; set `max_features` to bound); drift detectors O(1) or O(window_size); LinUCB O(arm_count × d²).

FTRL's `resource_diagnostics()` reports the current feature count, configured
cap, saturation, and new-feature rejection signal without mutating the model.
The unbounded default preserves compatibility; long-running services that
accept untrusted or open-ended vocabularies should configure a finite
`max_features`. See [`docs/FTRL_RESOURCE_DIAGNOSTICS.md`](docs/FTRL_RESOURCE_DIAGNOSTICS.md).

Delayed feedback uses a caller-owned clock, explicit model generations,
bounded pending/completed storage, and exact replay semantics; `DecisionLedger`
never silently evicts state. See [`docs/DECISION_LEDGER_V1.md`](docs/DECISION_LEDGER_V1.md).

## 4. Architecture

RillML is uniformly expressed as three layers: the Online ML core, the Runtime, and the Integrations layer.

```text
                       RillML
                         │
          ┌──────────────┼──────────────┐
          │              │              │
       Online ML      Runtime        Integrations
          │              │              │
    statistics         IPC             PM
    regression         packs           Xray
    classification     handlers        native apps
    drift              signing
    bandits            sandbox
    decisions          state
```

Or, from the host perspective:

```text
Host Application
      │
      ▼
RillML Runtime
      │
      ├── Online ML Core
      ├── State / Snapshot
      ├── Drift / Decision
      ├── Signed Model Packs
      └── Sandboxed Handlers
```

`rill-ml` is the RillML adaptive intelligence core library; it forms the foundation of the runtime but is not the whole project.

## 5. Quick start

### Installation

```bash
cargo add rill-ml
```

For serialization support, enable the `serde` feature:

```bash
cargo add rill-ml --features serde
```

`rill-ml` enables the `bandit` feature (multi-armed bandits module, pulls in `rand`) by default;
disable it with `cargo add rill-ml --no-default-features` if not needed.

`rill-runtime` enables the `wasm` feature (Wasmtime sandbox, can load `.rillhandler` packs out
of the box) by default, so `cargo install rill-runtime` matches the official GitHub binary
behaviour. For a WASM-free build: `cargo install rill-runtime --no-default-features` (cannot
load `.rillhandler`).

The version tracks `[workspace.package].version`; query the current release via `cargo metadata`
or [`CHANGELOG.md`](CHANGELOG.md). The Stable group is currently at `1.5.1`;
the Preview group remains at `0.15.0`.

**Requirements:** Rust 1.94+ (Edition 2024), no nightly needed.

### Basic usage

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

### Examples

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

## 6. RillML Runtime

`rill-runtime` (display name **RillML Runtime**, shorthand **Rill Runtime**) is a standalone local inference product built on top of `rill-ml`. A host application only needs to compile the small `rill-runtime-protocol`; it does not link the RillML engine into itself, so the Runtime and model packs can be updated independently of the host application.

| Crate | Description | Status | Install |
|---|---|---|---|
| `rill-runtime` | Standalone executable runtime that loads signed model and handler packs | Stable | `cargo install rill-runtime` |
| `rill-runtime-protocol` | Stable, strict, versioned JSON IPC types | Stable | `cargo add rill-runtime-protocol` |
| `rill-handler-api` | Versioned WIT handler ABI contract (for handler authors) | Stable | `cargo add rill-handler-api` |

The complete runtime product boundary, IPC versions, CLI, and release contract are documented in [`RUNTIME.md`](RUNTIME.md).

## 7. Online ML

### Progressive evaluation

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

### Serialization

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

## 8. Signed model and handler assets

RillML's model packs, handler packs, and release index are all signed, verifiable, and rollback-able:

- `.rillpack` model packs: model definition, parameters, checksum, and Ed25519 signature.
- `.rillhandler` handler packs: WASM handler module, manifest, checksum, and Ed25519 signature.
- `stable-index.json`: versions, platforms, URLs, sizes, and SHA-256 for the Runtime/models/handlers, signed by Ed25519.

Models and handlers use independent trust stores and cannot be merged automatically; model keys cannot sign handlers and vice versa. The release index signature covers the SHA-256, size, version, platform, and URL of every binary, model pack, and handler pack.

## 9. Safety model

RillML follows the principle "the more capable, the clearer the permission boundary":

- Models and handlers can never modify host state directly (no UCI writes, no sysctl writes, no firewall changes, no arbitrary command execution).
- The Runtime executes handlers inside a Wasmtime sandbox: no WASI permissions, per-call fuel budget and epoch timeout, 64 MiB memory cap, and a 1 MiB JSON I/O cap.
- Startup self-checks, signature verification, SHA-256 verification, size validation, HTTPS policy, message bounds, and fail-closed fallback are enforced across all asset loading paths.

The complete threat model and safe usage guidance are documented in [`SECURITY.md`](SECURITY.md).

## 10. Platform support

| Platform | Runtime | Core library |
|---|---|---|
| Linux x86_64 | Stable | Stable |
| Windows x86_64 | Stable | Stable |
| Windows ARM64 (aarch64) | Stable | Stable |
| macOS aarch64 (Apple Silicon) | Stable (unsigned) | Stable |
| macOS x86_64 (Intel) | Not released | Stable |

Official macOS Runtime assets are Apple Silicon (ARM64) only, under the permanent unsigned policy; Linux and Windows remain x86_64. The full matrix and Stable/Preview commitments are documented in [`PLATFORM_SUPPORT.md`](PLATFORM_SUPPORT.md).

## 11. Integrations and ecosystem

Workspace crates are split into a Stable group (under the 1.x compatibility freeze) and a Preview group (still at `0.x`). The full stability matrix and frozen surface are documented in [`STABILITY.md`](STABILITY.md). The core library does not pull in `tokio`/`arrow`/`polars`/`wasm-bindgen`/`pyo3` by default.

| Crate | Description | Status | Install |
|---|---|---|---|
| `rill-ml` | RillML adaptive intelligence core library (the subject of this document) | Stable | `cargo add rill-ml` |
| `rill-ml-tokio` | Drives `predict → metric → learn` over a `tokio_stream::Stream` | Preview | `cargo add rill-ml-tokio` |
| `rill-ml-arrow` | Convert between Apache Arrow `RecordBatch`/`Float64Array` and `&[f64]` | Preview | `cargo add rill-ml-arrow` |
| `rill-ml-polars` | Convert between Polars `DataFrame` and sample pairs; append prediction column | Preview | `cargo add rill-ml-polars` |
| `rill-ml-wasm` | WebAssembly bindings (`wasm32-unknown-unknown`) for browser-side online learning | Preview | `cargo add rill-ml-wasm` |
| `rill-ml-python` | Python bindings (PyO3 + Maturin); PyPI package `rill-ml-python`, `import rill_ml` | Preview | `pip install rill-ml-python` |
| `rillml-inspect` | CLI to view `Snapshot` JSON, version, and validation status (not a runtime dependency) | Preview | `cargo install rillml-inspect` |
| `rill-ml-ffi` | Stable C ABI (opaque handle + `rill_ml.h`) for C/C++, Android (JNI), iOS (Swift) | Preview | `cargo add rill-ml-ffi` |
| `rill-pm-adapter` | `pm-rill-shadow` v1 decision adapter for the OpenWrt Performance Manager (advisory only; does not compile RillML) | Preview | — |

### Related projects

| Project | Focus | Relationship to RillML |
|---|---|---|
| [River](https://riverml.xyz/) | Python online learning | RillML is inspired by its workflow, independently implemented, no compatibility target |
| [Linfa](https://github.com/rust-ml/linfa) | Rust batch learning toolkit | Batch-focused; RillML focuses on online/incremental learning |
| [SmartCore](https://smartcorelib.org/) | Rust ML library | Primarily batch-oriented; RillML targets streaming and edge deployment |
| [Burn](https://burn-rs.github.io/) | Rust deep learning framework | Targets neural networks and GPU; RillML targets lightweight online models |

These projects are complementary, not competitive.

## 12. Stability

RillML is under the 1.x Stable compatibility commitment. The following contracts remain frozen within 1.x:

```text
rill-ml Rust public API
selected serde model state
rill-runtime-protocol IPC v1/v2
rill-handler-api WIT ABI v1
rill-runtime public API / CLI
.rillpack format v1
.rillhandler format v1
stable release-index contract
```

Stable crate public APIs are recorded in `api-baseline/` and enforced by `cargo-semver-checks` in CI. Do not force a 2.0 for positioning/documentation upgrades: only a real API / ABI / wire / state schema breaking change warrants 2.0. The full policy is documented in [`STABILITY.md`](STABILITY.md).

## 13. Roadmap

RillML follows a real-need-driven roadmap. See [`ROADMAP.md`](ROADMAP.md) for the full plan and [`STABILITY.md`](STABILITY.md) for the 1.0 compatibility commitments.

- **v0.1** — Basic closed loop: predict, evaluate, learn, save, restore.
- **v0.2** — Reliability and diagnostics: prediction reports, cold-start, baseline comparison.
- **v0.3** — Sparse features and high-dimensional data: FeatureHasher, FTRL, Naive Bayes.
- **v0.4** — Drift detection: Page-Hinkley, ADWIN, KSWIN, adaptive learning.
- **v0.5** — Online decision-making: multi-armed bandits, contextual bandits.
- **v0.6** — Platform and ecosystem: WASM, Python bindings, Tokio Stream adapters.
- **v0.7** — Pluggable WASM handlers: signed `.rillhandler` packs, Wasmtime sandbox, IPC v2.
- **v0.13.0** — Last 0.x stable release.
- **v1.0.0-rc.1** — Start of the 1.0 candidate cycle: API, state format, IPC, WIT, runtime, and release channels were frozen.
- **v1.0.0** — First stable 1.x release; `local-ai-stable` advances only after the immutable public assets pass the independent host smoke.
- **v1.1.0** — Interpretable online decision-making: delayed feedback, feature identity, drift consensus, weighted learning, Preview Runtime v3 / Handler v2.
- **v1.2.0** — Release admission and full-platform post-verification: admission gate, native RISC-V/LoongArch/FreeBSD/Windows verification, index schema v3 with `targetLibc`.
- **v1.3.0** — Product-surface drift gates, Stable/Preview version convergence, TrustMetadata v1 key lifecycle, offline/released conformance, real fuzz harnesses, CycloneDX/SPDX SBOMs, and FTRL resource diagnostics.
- **v1.5.0** — Stateful Runtime v3 production qualification boundaries, hard resource limits, structured health/diagnostics, state rollback, and post-push simulated consumer/fault qualification harnesses.

The current 1.x line does not implement full deep learning; the future direction is "frozen neural encoder + RillML online adaptive head" (e.g., DL embedding → LinearRegression / LogisticRegression / LinUCB / Bandit → online adaptation), with an `inference-provider` abstraction reserved in the architecture. RillML is not intended to become a PyTorch/tinygrad replacement.

## 14. Naming

The project is officially named **RillML**.

**Rill** is a short display and conversational name used where the context is unambiguous. The project does not claim the bare `rill` package, module, or CLI namespace. Existing technical names such as `rill-ml`, `rill-runtime`, and `rill_ml` remain intentionally explicit. See [`docs/NAMING.md`](docs/NAMING.md).

## Correctness and validation

RillML is validated through multiple layers:

- Unit, integration, and doctests cover core math, serialization round-trips, and boundary behaviour; current counts are visible in `cargo test --workspace` output and the CI summary.
- Serialization round-trip tests for all stateful types.
- `proptest` property-based tests with fixed seeds (`rand_chacha`).
- Clippy with `-D warnings` in CI; rustfmt enforced.
- All examples are actually run and verified.

**Numerical stability:** Welford's algorithm for variance; numerically stable sigmoid; epsilon-guarded scaling; no panics in public APIs, all errors returned as `Result<_, RillError>`.

## License

Licensed under the MIT License ([LICENSE-MIT](LICENSE-MIT)).

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) before submitting a pull request. RillML follows a "real-need-driven" development principle: every new feature should solve a real problem in a real Rust application.
