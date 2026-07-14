<p align="center">
  <img src="logo.png" alt="RillML" width="480">
</p>

<p align="center">
  面向 Rust 应用、边缘设备和持续变化数据流的轻量在线机器学习库
</p>

<p align="center">
  <a href="https://github.com/hello-yunshu/rill-ml/actions/workflows/ci.yml"><img src="https://github.com/hello-yunshu/rill-ml/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://crates.io/crates/rill-ml"><img src="https://img.shields.io/crates/v/rill-ml.svg" alt="crates.io"></a>
  <a href="https://docs.rs/rill-ml"><img src="https://docs.rs/rill-ml/badge.svg" alt="docs.rs"></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/crates/l/rill-ml.svg" alt="License: MIT"></a>
  <img src="https://img.shields.io/badge/rust-1.85%2B-orange.svg" alt="Rust 1.85+">
</p>

<p align="center">
  <a href="README.en.md">English</a> &middot; <a href="CHANGELOG.md">更新日志</a> &middot; <a href="ROADMAP.md">路线图</a> &middot; <a href="https://docs.rs/rill-ml">API 文档</a>
</p>

---

RillML 提供可直接嵌入 Rust 原生应用的增量学习组件：在线统计、预处理器、线性/逻辑回归、评估指标、Pipeline、渐进式评估，以及基于 serde 的可选状态持久化。

Workspace 还包含可独立分发的 `rill-runtime`、稳定 IPC 约定和签名 `.rillpack` 模型包。宿主可以只依赖协议 crate，让 Runtime 与模型脱离主程序单独更新。详见 [`RUNTIME.md`](RUNTIME.md)。

> RillML 受 [River](https://riverml.xyz/) 推广的在线学习工作流启发，是独立的 Rust 项目，与 River 无关联，目前不追求 API 或模型兼容性。

## 为什么需要在线学习？

传统机器学习采用批量工作流：收集数据、离线训练、部署固定模型、定期重训练。这在数据充足、稳定且可集中获取时表现良好。

在线学习采用不同方式：**逐条处理样本，先预测后学习，持续适应**。适用于：

- **流式数据** — 无法存储全部历史。
- **边缘设备** — 内存有限，无 Python 运行时。
- **持续变化的环境** — 固定模型会逐渐失效。
- **隐私敏感场景** — 数据不应离开设备。
- **实时系统** — 需要在下一条样本到达前给出预测。

RillML 用纯安全 Rust 实现这一工作流，内存有界。

## 适用场景

- IoT 遥测、资源用量、传感器读数等在线回归任务。
- 基于滚动统计的传感器异常检测。
- 实时点击或事件分类。
- 存在概念漂移的网络延迟预测。
- 任何需要轻量、持续学习组件的 Rust 应用。

**非适用场景：** 大规模离线训练（用 Linfa/SmartCore/Python）、深度学习（用 Burn/candle/tch-rs）、分布式训练、GPU 加速、研究实验（Python 更合适）。Rust 不会让同一算法天然更准确，价值主要来自工程部署、状态管理和本地运行。

## 安装

```toml
[dependencies]
rill-ml = "0.6"
```

需要序列化支持时启用 `serde` feature：

```toml
[dependencies]
rill-ml = { version = "0.6", features = ["serde"] }
```

**环境要求：** Rust 1.85+（Edition 2024），无需 nightly。

## 快速开始

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

## 渐进式评估

在线学习的核心契约是：**先预测，后学习**。`evaluate` 模块强制执行以下顺序：

```text
predict  →  metric.update  →  learn
```

这确保指标反映模型对**未见过**数据的泛化能力，而非对已记忆样本的拟合。

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

## 示例

| 示例 | 说明 | 运行命令 |
|---|---|---|
| [online_regression](examples/online_regression.rs) | 对比 Mean/EWMean/LinearRegression，演示 StandardScaler 与 Snapshot 序列化 | `cargo run --example online_regression --features serde` |
| [online_classification](examples/online_classification.rs) | LogisticRegression 在线二分类 | `cargo run --example online_classification` |
| [diagnostics_demo](examples/diagnostics_demo.rs) | TrainingSummary、PredictionReporter、OnlineModelSelector、ModelHealthReport | `cargo run --example diagnostics_demo` |
| [sparse_classification](examples/sparse_classification.rs) | SparseFeatures、FeatureHasher、FTRL、NaiveBayes 高维稀疏分类 | `cargo run --example sparse_classification` |
| [drift_demo](examples/drift_demo.rs) | Page-Hinkley、ADWIN、KSWIN 漂移检测与 DriftAwareModel | `cargo run --example drift_demo` |
| [bandit_demo](examples/bandit_demo.rs) | EpsilonGreedy、UCB1、ThompsonSampling、LinUCB 在线决策 | `cargo run --example bandit_demo` |
| [sensor_stream](examples/sensor_stream.rs) | 传感器数据流在线统计 | `cargo run --example sensor_stream` |
| [progressive_validation](examples/progressive_validation.rs) | 渐进式评估流程演示 | `cargo run --example progressive_validation` |

## 序列化

启用 `serde` feature 后可以序列化和恢复模型状态：

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

`Snapshot<T>` 使用格式版本号包裹模型状态，并拒绝不兼容的版本。快照来源不可信或模型还有业务约束时，请使用 `into_model_with_validation()` 在启用恢复状态前执行应用级校验。完整的生产接入与故障回退建议见 [`RELIABILITY.md`](RELIABILITY.md)。

## 模块总览（v0.6）

| 类别 | 模块 |
|---|---|
| 统计 | Mean, Variance, Std, Count, Sum, Min, Max, EWMean, RollingMean, RollingVariance |
| 预处理 | StandardScaler, MinMaxScaler, Clipper, OneHotEncoder, OrdinalEncoder, FrequencyEncoder, MissingIndicator, ConstantImputer, MeanImputer, ForwardFill |
| 稀疏特征 | SparseFeatures, FeatureHasher |
| 模型 | LinearRegression, LogisticRegression, MeanRegressor, EWMeanRegressor, LastValueRegressor, FtrlRegressor, FtrlClassifier, GaussianNaiveBayes, BernoulliNaiveBayes, MultinomialNaiveBayes |
| 优化器 | SGD（含 L2）, AdaGrad |
| 损失函数 | SquaredError, HuberLoss, BinaryLogLoss |
| 回归指标 | MAE, MSE, RMSE, R², RollingMAE, RollingMSE |
| 分类指标 | Accuracy, Precision, Recall, F1, LogLoss, RollingAccuracy |
| Pipeline | RegressionPipeline, ClassificationPipeline |
| 评估 | 渐进式评估（predict → metric → learn） |
| 持久化 | `Snapshot<T>` 版本化封装（serde feature） |
| 诊断 | TrainingSummary, WarmupTracker, BaselineComparator, OnlineModelSelector, ResidualInterval, ModelHealthReport, PredictionReporter |
| 漂移检测 | PageHinkley, Adwin, Kswin, DriftAwareModel, DriftAction, DriftStrategy, TimeDecayedMean, LearningRateScheduler, FixedWindowBuffer |
| 在线决策 | EpsilonGreedy, Ucb1, ThompsonSampling, LinUcb, ArmStats |

**内存界限：** 非滚动统计量 O(1)；线性模型 O(d)；滚动统计量 O(window_size)；稀疏模型（FTRL）O(k)，k 为已见特征数；漂移检测器 O(1) 或 O(window_size)；LinUCB O(arm_count × d²)。

## 生态与平台扩展（v0.6）

v0.6 新增五个独立可发布的 crate，均位于 `crates/` 下，依赖 `rill-ml` 但不改变核心 API。核心库默认不引入 `tokio`/`arrow`/`polars`/`wasm-bindgen`/`pyo3`。

| Crate | 说明 | 安装 |
|---|---|---|
| `rill-ml-tokio` | 在 `tokio_stream::Stream` 上驱动 `predict → metric → learn` | `cargo add rill-ml-tokio` |
| `rill-ml-arrow` | Apache Arrow `RecordBatch`/`Float64Array` 与 `&[f64]` 互转 | `cargo add rill-ml-arrow` |
| `rill-ml-polars` | Polars `DataFrame` 与样本对互转，追加预测列 | `cargo add rill-ml-polars` |
| `rillml-inspect` | 查看 `Snapshot` JSON、版本与校验的 CLI（非运行依赖） | `cargo install rillml-inspect` |
| `rill-ml-wasm` | WebAssembly 绑定（`wasm32-unknown-unknown`），浏览器端在线学习 | `cargo add rill-ml-wasm` |
| `rill-ml-python` | Python 绑定（PyO3 + Maturin），PyPI 包名 `rill-ml-python`，`import rill_ml` | `pip install rill-ml-python` |

## 路线图

RillML 遵循真实需求驱动的路线图。完整规划参见 [`ROADMAP.md`](ROADMAP.md)。

- **v0.1** — 基础闭环：预测、评估、学习、保存、恢复。
- **v0.2** — 可靠性与诊断：预测报告、冷启动、基线比较。
- **v0.3** — 稀疏特征与高维数据：FeatureHasher、FTRL、朴素贝叶斯。
- **v0.4** — 漂移检测：Page-Hinkley、ADWIN、KSWIN、自适应学习。
- **v0.5** — 在线决策：多臂老虎机、上下文老虎机。
- **v0.6** — 平台与生态：WASM、Python 绑定、Tokio Stream 适配。*（当前）*
- **v1.0** — 稳定的 API 和状态格式。

## 正确性与验证

RillML 通过多层验证保证正确性：

- **562** 个单元测试 + **130** 个集成测试 + **40** 个文档测试。
- 序列化往返测试覆盖所有有状态类型。
- `proptest` 性质测试与固定随机种子（`rand_chacha`）确定性测试。
- CI 中以 `-D warnings` 强制 Clippy，rustfmt 强制执行。
- 所有示例实际运行通过。

**数值稳定性：** Welford 算法计算方差；数值稳定的 sigmoid；带 epsilon 保护的缩放；公共 API 不 panic，所有错误以 `Result<_, RillError>` 返回。

## 相关项目

| 项目 | 定位 | 与 RillML 的关系 |
|---|---|---|
| [River](https://riverml.xyz/) | Python 在线学习 | RillML 受其工作流启发，独立实现，不追求兼容 |
| [Linfa](https://github.com/rust-ml/linfa) | Rust 批量学习工具集 | 侧重批量学习；RillML 侧重在线/增量学习 |
| [SmartCore](https://smartcorelib.org/) | Rust 机器学习库 | 主要面向批量学习；RillML 面向流式和边缘部署 |
| [Burn](https://burn-rs.github.io/) | Rust 深度学习框架 | 面向神经网络和 GPU；RillML 面向轻量在线模型 |

这些项目是互补的，而非竞争关系。

## 命名说明

本项目名为 **RillML**。与 [Rill Data](https://www.rilldata.com/) 或任何名为 "Rill" 的产品无关，也未获得其认可。RillML 不提供名为 `rill` 的 CLI 工具。

## 许可证

MIT 许可证（[LICENSE-MIT](LICENSE-MIT)）。

## 贡献

欢迎贡献。提交 Pull Request 前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。RillML 遵循"真实需求驱动"的开发原则：每个新功能都应解决真实 Rust 应用中的实际问题。
