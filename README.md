# RillML

面向 Rust 应用、边缘设备和持续变化数据流的轻量在线机器学习库。

RillML 提供可直接嵌入 Rust 原生应用的增量学习组件：在线统计、预处理器、线性/逻辑回归、评估指标、Pipeline、渐进式评估，以及基于 serde 的可选状态持久化。

> RillML 受 [River](https://riverml.xyz/) 推广的在线学习工作流启发。它是一个独立的 Rust 项目，与 River 无关联，也未获得 River 的认可。目前不追求 API 或模型兼容性。

---

## 为什么需要在线学习？

传统机器学习采用批量工作流：收集数据、离线训练、部署固定模型、定期重训练。这在数据充足、稳定且可集中获取时表现良好。

在线学习采用不同方式：逐条处理样本，先预测后学习，持续适应。适用于：

- **流式数据**：无法存储全部历史。
- **边缘设备**：内存有限，无 Python 运行时。
- **持续变化的环境**：固定模型会逐渐失效。
- **隐私敏感场景**：数据不应离开设备。
- **实时系统**：需要在下一条样本到达前给出预测。

RillML 用纯安全 Rust 实现这一工作流，内存有界。

---

## 适用场景

- IoT 遥测、资源用量、传感器读数等在线回归任务。
- 基于滚动统计的传感器异常检测。
- 实时点击或事件分类。
- 存在概念漂移的网络延迟预测。
- 任何需要轻量、持续学习组件的 Rust 应用。

## 非适用场景

- 大规模离线模型训练（请使用 Linfa、SmartCore 或 Python）。
- 深度学习（请使用 Burn、candle 或 tch-rs）。
- 跨多机的分布式训练。
- 需要 GPU 加速的场景。
- 研究和快速算法实验（Python 更合适）。

Python 更适合研究、数据分析和快速算法实验。RillML 重点解决 Rust 原生嵌入和持续运行。Rust 不会让同一算法天然更准确，价值主要来自工程部署、状态管理和本地运行。

---

## 安装

在 `Cargo.toml` 中添加：

```toml
[dependencies]
rill-ml = "0.1"
```

需要序列化支持时启用 `serde` feature：

```toml
[dependencies]
rill-ml = { version = "0.1", features = ["serde"] }
```

**环境要求：** Rust 1.85+（Edition 2024），无需 nightly。

---

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

---

## 渐进式评估

在线学习的核心契约是：**先预测，后学习**。RillML 的 `evaluate` 模块强制执行以下顺序：

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

---

## 回归示例

参见 [`examples/online_regression.rs`](examples/online_regression.rs)，完整的在线回归演示：
- 对比 `MeanRegressor`、`EWMeanRegressor` 和 `LinearRegression`。
- 使用 `StandardScaler` 进行特征标准化。
- 演示 `Snapshot` 序列化往返。

```sh
cargo run --example online_regression --features serde
```

---

## 分类示例

参见 [`examples/online_classification.rs`](examples/online_classification.rs)，使用 `LogisticRegression` 进行在线二分类：

```sh
cargo run --example online_classification
```

---

## 诊断示例

参见 [`examples/diagnostics_demo.rs`](examples/diagnostics_demo.rs)，演示 v0.2 诊断模块：

- 使用 `TrainingSummary` 跟踪训练统计。
- 使用 `PredictionReporter` 生成带置信度和预测区间的报告。
- 使用 `OnlineModelSelector` 比较 `MeanRegressor` 与 `LinearRegression` 并自动选择最佳模型。
- 使用 `ModelHealthReport` 检测参数中的 NaN/Infinity。

```sh
cargo run --example diagnostics_demo
```

---

## 稀疏特征示例

参见 [`examples/sparse_classification.rs`](examples/sparse_classification.rs)，演示高维稀疏分类：

- 使用 `SparseFeatures` 表示稀疏特征向量。
- 使用 `FeatureHasher` 将字符串特征名哈希为 `FeatureId`。
- 对比 `FtrlClassifier`（稀疏输入）与 `LogisticRegression`（哈希后稠密输入）与 `GaussianNaiveBayes`。
- 演示 FTRL 的 L1 正则化产生的稀疏权重。

```sh
cargo run --example sparse_classification
```

`SparseFeatures` 使用排序的 `Vec<(u64, f64)>` 而非 `HashMap`，支持二分查找和确定性序列化。`FtrlRegressor` / `FtrlClassifier` 通过 `BTreeMap<FeatureId, FtrlParam>` 实现动态特征增长，无需预先知道所有特征 ID。

---

## 漂移检测示例

参见 [`examples/drift_demo.rs`](examples/drift_demo.rs)，演示 v0.4 漂移检测模块：

- 使用 `PageHinkley` 检测均值偏移。
- 使用 `Adwin` 检测自适应窗口分布变化。
- 使用 `Kswin` 检测分布形状变化。
- 演示 `DriftAwareModel` 在检测到漂移时自动重置 `LinearRegression`。

```sh
cargo run --example drift_demo
```

---

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

`Snapshot<T>` 使用格式版本号包裹模型状态，支持前向兼容。

---

## 基线模型

RillML 提供三个简单的基线回归器：

- **`MeanRegressor`** — 预测所有已见目标的运行均值。
- **`ExponentiallyWeightedMeanRegressor`** — 对近期目标赋予更高权重。
- **`LastValueRegressor`** — 预测上一个见到的目标。

始终使用渐进式评估将模型与基线比较。只有当复杂模型持续优于基线时，才应信任复杂模型。

---

## 当前范围（v0.4）

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

内存界限：
- 非滚动统计量：O(1)
- 线性模型：O(d)，d 为特征数
- 滚动统计量：O(window_size)
- 诊断组件：O(1) 或 O(window_size)，不存储原始样本
- 稀疏模型（FTRL）：O(k)，k 为已见特征数（非特征空间总量）
- 分类编码器：O(c)，c 为已见类别数
- 漂移检测器：O(1)（PageHinkley）或 O(window_size)（Adwin/Kswin）
- DriftAwareModel：O(max_events) 事件日志 + 模型 + 检测器

---

## 路线图

RillML 遵循真实需求驱动的路线图。完整规划参见 [RillML_Roadmap.md](RillML_Roadmap(1).md)。

- **v0.1** — 基础闭环：预测、评估、学习、保存、恢复。
- **v0.2** — 可靠性与诊断：预测报告、冷启动、基线比较。
- **v0.3** — 稀疏特征与高维数据：FeatureHasher、FTRL、朴素贝叶斯。
- **v0.4** — 漂移检测：Page-Hinkley、ADWIN、KSWIN、自适应学习。*（当前）*
- **v0.5** — 在线决策：多臂老虎机、上下文老虎机。
- **v0.6** — 平台与生态：WASM、Python 绑定、Tokio Stream 适配。
- **v1.0** — 稳定的 API 和状态格式。

---

## 正确性与验证

RillML 通过多层验证保证正确性：

- **单元测试**：每个模块的单元测试，共 451 个。
- **集成测试**：112 个集成测试，将在线算法与批量参考公式对照。
- **Doctest**：所有公共 API 均有文档测试，共 31 个。
- **序列化往返测试**：所有有状态类型的序列化/反序列化验证。
- **性质测试**：使用 `proptest`。
- **确定性测试**：使用固定随机种子（`rand_chacha`）。
- **Clippy**：CI 中以 `-D warnings` 强制。
- **rustfmt**：强制执行。
- **示例运行验证**：所有示例均实际运行通过。

数值稳定性：
- Welford 算法计算方差。
- 数值稳定的 sigmoid。
- 带 epsilon 保护的缩放，避免除零。
- 公共 API 不 panic，所有错误以 `Result<_, RillError>` 返回。

---

## 与 River 的关系

RillML 受 [River](https://riverml.xyz/) 推广的在线学习工作流启发。它是一个独立的 Rust 项目，与 River 无关联，也未获得 River 的认可。目前不追求 API 或模型兼容性。

River 在 Python 在线学习研究和实验方面仍然是优秀的选择。

---

## 与 Linfa、SmartCore、Burn 的关系

- **[Linfa](https://github.com/rust-ml/linfa)** — 受 scikit-learn 启发的 Rust 机器学习工具集，侧重批量学习。RillML 侧重内存有界的在线/增量学习。
- **[SmartCore](https://smartcorelib.org/)** — 快速的 Rust 机器学习库，算法覆盖广泛，主要面向批量学习。RillML 面向流式数据和边缘部署。
- **[Burn](https://burn-rs.github.io/)** — Rust 深度学习框架，面向神经网络和 GPU 计算。RillML 面向可在任何地方运行的轻量在线模型。

这些项目是互补的，而非竞争关系。RillML 不旨在替代它们。

---

## 命名说明

本项目名为 **RillML**。与 [Rill Data](https://www.rilldata.com/) 或任何名为 "Rill" 的产品无关，也未获得其认可。RillML 不提供名为 `rill` 的 CLI 工具。

---

## 许可证

MIT 许可证（[LICENSE-MIT](LICENSE-MIT)）。

---

## 贡献

欢迎贡献。提交 Pull Request 前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。

RillML 遵循"真实需求驱动"的开发原则：每个新功能都应解决真实 Rust 应用中的实际问题，而非仅仅复制其他框架中已有的模块。优先方向参见[路线图](RillML_Roadmap(1).md)。
