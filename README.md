<p align="center">
  <img src="logo.png" alt="RillML" width="480">
</p>

<p align="center">
  <strong>RillML（简称 Rill）</strong> 是面向原生应用与边缘设备的轻量自适应智能运行时。
</p>

<p align="center">
  它将在线机器学习、有界自适应、稳定运行时协议、签名模型/处理器资产以及安全宿主集成组合在一起，用于持续变化的系统环境。
</p>

<p align="center">
  <a href="https://github.com/hello-yunshu/rill-ml/actions/workflows/pipeline.yml"><img src="https://github.com/hello-yunshu/rill-ml/actions/workflows/pipeline.yml/badge.svg" alt="CI / Release"></a>
  <a href="https://crates.io/crates/rill-ml"><img src="https://img.shields.io/crates/v/rill-ml.svg" alt="crates.io"></a>
  <a href="https://docs.rs/rill-ml"><img src="https://docs.rs/rill-ml/badge.svg" alt="docs.rs"></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/crates/l/rill-ml.svg" alt="License: MIT"></a>
  <img src="https://img.shields.io/badge/rust-1.94%2B-orange.svg" alt="Rust 1.94+">
</p>

<p align="center">
  <a href="README.en.md">English</a> &middot; <a href="CHANGELOG.md">更新日志</a> &middot; <a href="ROADMAP.md">路线图</a> &middot; <a href="PLATFORM_SUPPORT.md">平台支持</a> &middot; <a href="https://docs.rs/rill-ml">API 文档</a>
</p>

---

## 1. RillML 简介

**RillML**（简称 **Rill**）是面向原生应用与边缘设备的轻量自适应智能运行时（lightweight adaptive intelligence runtime）。

它不是"又一个在线机器学习库"，而是把以下能力组合为一个可独立分发的运行时：

- **在线机器学习核心**：统计、回归/分类、漂移检测、多臂老虎机与决策原语，全部用纯安全 Rust 实现，状态有界；
- **RillML Runtime（`rill-runtime`）**：独立的本地推理运行时，在沙箱内加载签名验证的 WASM handler，宿主无需链接引擎即可安全集成；
- **签名资产**：`.rillpack` 模型包、`.rillhandler` handler 包与 Ed25519 签名的发布索引，保证每次更新可验证、可回滚；
- **稳定协议边界**：`rill-runtime-protocol` 提供 v1/v2 冻结的 JSON IPC；`rill-handler-api` 提供 v1 冻结的 WIT ABI；
- **安全宿主集成**：模型与 handler 永远无法直接修改宿主状态，所有系统变更都归宿主治理层负责。

Runtime 的 Stable CLI `serve` 仍仅接受 Stable IPC v1/v2；Preview IPC v3 通过
明确 opt-in 的 `preview-serve --state PATH` subprocess 入口提供，握手带有
机器可读的 `channel: "preview"`，不会改变 `serve` 的默认语义。`serve` 必须显式
提供签名 `--handler` 或已弃用的 `--builtin-handler linear-regression`，
缺少两者时会 fail-closed，不会隐式回退。

Workspace 还包含可独立分发的 `rill-runtime`、稳定 IPC 约定、签名 `.rillpack` 模型包和签名 `.rillhandler` WASM handler 包。Runtime 在沙箱内加载经过签名验证的 WASM handler，更新 handler 不需要重新编译 runtime 二进制。宿主可以只依赖协议 crate，让 Runtime、模型和 handler 各自独立更新。官方 macOS Runtime 仅发布 Apple Silicon（ARM64）版本，不提供 Intel 构建。macOS aarch64 资产为未签名（unsigned）形态——项目不配置 Apple Developer ID 证书，这是永久决策，不影响发布。macOS 用户首次运行可能需要在 Finder 中右键选择"打开"，或在"系统设置 → 隐私与安全性"中允许，详见 [`STABILITY.md`](STABILITY.md) § macOS unsigned policy。1.0 稳定性矩阵、冻结范围与 Stable/Preview 划分详见 [`STABILITY.md`](STABILITY.md)；运行时产品边界详见 [`RUNTIME.md`](RUNTIME.md)。

> RillML 受 [River](https://riverml.xyz/) 推广的在线学习工作流启发，是独立的 Rust 项目，与 River 无关联，目前不追求 API 或模型兼容性。

## 2. 为什么选择 RillML

### 为什么需要在线学习？

传统机器学习采用批量工作流：收集数据、离线训练、部署固定模型、定期重训练。这在数据充足、稳定且可集中获取时表现良好。

在线学习采用不同方式：**逐条处理样本，先预测后学习，持续适应**。适用于：

- **流式数据** — 无法存储全部历史。
- **边缘设备** — 内存有限，无 Python 运行时。
- **持续变化的环境** — 固定模型会逐渐失效。
- **隐私敏感场景** — 数据不应离开设备。
- **实时系统** — 需要在下一条样本到达前给出预测。

RillML 用纯安全 Rust 实现这一工作流；固定维度算法使用有界状态，FTRL 等动态特征算法需设置 `max_features` 才能保证状态有界。

### 适用场景

- IoT 遥测、资源用量、传感器读数等在线回归任务。
- 基于滚动统计的传感器异常检测。
- 实时点击或事件分类。
- 存在概念漂移的网络延迟预测。
- 任何需要轻量、持续学习组件的 Rust 应用。

**非适用场景：** 大规模离线训练（用 Linfa/SmartCore/Python）、深度学习（用 Burn/candle/tch-rs）、分布式训练、GPU 加速、研究实验（Python 更合适）。Rust 不会让同一算法天然更准确，价值主要来自工程部署、状态管理和本地运行。

### 产品边界

RillML 的核心价值不是"算法数量最多"，而是：

```text
online adaptation  有界自适应
bounded resource   有界资源使用
native/edge deploy 原生/边缘部署
stable protocols   稳定协议边界
signed artifacts   签名资产
safe execution     安全运行时执行
state lifecycle    状态生命周期
feedback loop      反馈/结果闭环
drift handling     漂移处理
decision primitives 决策原语
product integration 产品集成
```

与 River 的定位关系：

```text
River   = 广泛在线 ML 算法与实验生态
RillML  = 面向原生与边缘系统的生产级自适应智能运行时
```

可以把 River 作为参考、基线与离线验证来源，但不引入 Python 作为目标设备的生产依赖。

## 3. 核心能力

### 模块总览

| 类别 | 模块 |
|---|---|
| 统计 | Mean, Variance, Std, Count, Sum, Min, Max, EWMean, RollingMean, RollingVariance, P2Quantile, ClippedMean, bounded RollingMedianMad/modified-z, weighted statistics |
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
| 漂移检测 | PageHinkley, Adwin, Kswin, portable state V1, DriftConsensus, DriftAwareModel, DriftAction, DriftStrategy |
| 在线决策 | EpsilonGreedy, Ucb1, ThompsonSampling, LinUcb score breakdown, Preview LinUcbFast, DecisionLedger, DecisionReplayHarness |
| 身份与权重 | FeatureSchema, ModelDescriptor, WeightedStatistic/Regressor/Classifier |

**内存界限：** 非滚动统计量 O(1)；线性模型 O(d)；滚动统计量 O(window_size)；稀疏模型（FTRL）O(k)，k 为已见特征数（默认无上限，需设置 `max_features` 才能有界）；漂移检测器 O(1) 或 O(window_size)；LinUCB O(arm_count × d²)。

FTRL 的 `resource_diagnostics()` 只读报告当前特征数、有限上限、饱和度和
新特征拒绝信号；它不会替代 `learn` 的准入策略。无界默认值是兼容性选择，
面向不可信或开放词表输入的长期服务应配置有限 `max_features`。详见
[`docs/FTRL_RESOURCE_DIAGNOSTICS.md`](docs/FTRL_RESOURCE_DIAGNOSTICS.md)。

延迟反馈使用调用方时钟、显式 generation、有限 pending/completed 容量和
精确重放语义；`DecisionLedger` 不会隐式淘汰状态。详见
[`docs/DECISION_LEDGER_V1.md`](docs/DECISION_LEDGER_V1.md)。

## 4. 架构

RillML 统一表达为三层：在线机器学习核心、Runtime 与集成层。

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

或从宿主视角看：

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

`rill-ml` 是 RillML 的自适应智能核心库（adaptive intelligence core library），它构成整个运行时的基础，但不是项目的全部。

## 5. 快速开始

### 安装

```bash
cargo add rill-ml
```

需要序列化支持时启用 `serde` feature：

```bash
cargo add rill-ml --features serde
```

`rill-ml` 默认启用 `bandit` feature（多臂老虎机模块，引入 `rand`）；如不需可关闭默认特性：
`cargo add rill-ml --no-default-features`。

`rill-runtime` 默认启用 `wasm` feature（Wasmtime 沙箱，可直接加载 `.rillhandler` 包），
因此 `cargo install rill-runtime` 行为与官方 GitHub 二进制一致。如需 WASM-free 构建：
`cargo install rill-runtime --no-default-features`（无法加载 `.rillhandler`）。

版本号跟随 `[workspace.package].version`，可通过 `cargo metadata` 或 [`CHANGELOG.md`](CHANGELOG.md) 查询当前发布版本。当前 Stable 组版本为 `1.5.3`，Preview 组为 `0.15.0`。

**环境要求：** Rust 1.94+（Edition 2024），无需 nightly。

### 基本用法

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

### 示例

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

## 6. RillML Runtime

`rill-runtime`（正式展示名 **RillML Runtime**，简称 **Rill Runtime**）是建立在 `rill-ml` 之上的独立本地推理产品。宿主应用只需要编译很小的 `rill-runtime-protocol`，不需要把 RillML 引擎链接进自身，因此 Runtime 和模型包都能脱离宿主应用单独更新。

| Crate | 说明 | 状态 | 安装 |
|---|---|---|---|
| `rill-runtime` | 加载签名模型包与签名 handler 的独立可执行 runtime | Stable | `cargo install rill-runtime` |
| `rill-runtime-protocol` | 稳定、严格、带版本的 JSON IPC 类型 | Stable | `cargo add rill-runtime-protocol` |
| `rill-handler-api` | 版本化 WIT handler ABI 契约（handler 作者使用） | Stable | `cargo add rill-handler-api` |

完整的运行时产品边界、IPC 版本、CLI 与发布契约详见 [`RUNTIME.md`](RUNTIME.md)。

## 7. 在线机器学习

### 渐进式评估

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

### 序列化

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

## 8. 签名模型与处理器资产

RillML 的模型包、handler 包与发布索引都是签名、可验证、可回滚的：

- `.rillpack` 模型包：模型定义、参数、校验和与 Ed25519 签名。
- `.rillhandler` handler 包：WASM handler 模块、manifest、校验和与 Ed25519 签名。
- `stable-index.json`：Runtime/模型/handler 的版本、平台、URL、大小与 SHA-256，由 Ed25519 签名。

模型与 handler 使用独立的 trust store，不能自动合并；模型密钥不能签署 handler，反之亦然。发布索引的签名覆盖每个二进制、模型包和 handler 包的 SHA-256、大小、版本、平台与 URL。

## 9. 安全模型

RillML 遵循"能力越强、权限边界越清晰"的安全理念：

- 模型与 handler 永远无法直接修改宿主状态（不写 UCI、不写 sysctl、不改防火墙、不执行任意命令）。
- Runtime 在 Wasmtime 沙箱内执行 handler：无 WASI 权限、每次调用有独立 fuel 预算与 epoch 超时、内存上限 64 MiB、I/O JSON 上限 1 MiB。
- 启动自检、签名验证、SHA-256 验证、尺寸验证、HTTPS 策略、消息边界与 fail-closed 回退贯穿所有资产加载路径。

完整的威胁模型与安全使用建议见 [`SECURITY.md`](SECURITY.md)。

## 10. 平台支持

| 平台 | Runtime | 核心库 |
|---|---|---|
| Linux x86_64 | Stable | Stable |
| Windows x86_64 | Stable | Stable |
| Windows ARM64 (aarch64) | Stable | Stable |
| macOS aarch64 (Apple Silicon) | Stable (unsigned) | Stable |
| macOS x86_64 (Intel) | 不发布 | Stable |
| Linux riscv64 GNU | Stable | Stable |
| Linux musl x86_64/aarch64/riscv64/armv7/i686 | Stable | Stable |
| Linux armv7/s390x/powerpc64le GNU | 未列出（仅 Core） | Stable |

官方 macOS Runtime 资产仅 Apple Silicon（ARM64），且为永久未签名策略。通用 Linux/musl 支持规则见 [`docs/LINUX_MUSL_SUPPORT.md`](docs/LINUX_MUSL_SUPPORT.md)；OpenWrt 消费者资格见 [`docs/consumers/OPENWRT.md`](docs/consumers/OPENWRT.md)。

## 11. 集成与生态

Workspace 下的 crate 分为 Stable 组（1.x 兼容冻结）和 Preview 组（0.x）。完整稳定性矩阵与冻结范围见 [`STABILITY.md`](STABILITY.md)。核心库默认不引入 `tokio`/`arrow`/`polars`/`wasm-bindgen`/`pyo3`。

| Crate | 说明 | 状态 | 安装 |
|---|---|---|---|
| `rill-ml` | 核心自适应智能库（本文档主体） | Stable | `cargo add rill-ml` |
| `rill-ml-tokio` | 在 `tokio_stream::Stream` 上驱动 `predict → metric → learn` | Preview | `cargo add rill-ml-tokio` |
| `rill-ml-arrow` | Apache Arrow `RecordBatch`/`Float64Array` 与 `&[f64]` 互转 | Preview | `cargo add rill-ml-arrow` |
| `rill-ml-polars` | Polars `DataFrame` 与样本对互转，追加预测列 | Preview | `cargo add rill-ml-polars` |
| `rill-ml-wasm` | WebAssembly 绑定（`wasm32-unknown-unknown`），浏览器端在线学习 | Preview | `cargo add rill-ml-wasm` |
| `rill-ml-python` | Python 绑定（PyO3 + Maturin），PyPI 包名 `rill-ml-python`，`import rill_ml` | Preview | `pip install rill-ml-python` |
| `rillml-inspect` | 查看 `Snapshot` JSON、版本与校验的 CLI（非运行依赖） | Preview | `cargo install rillml-inspect` |
| `rill-ml-ffi` | 稳定 C ABI（opaque handle + `rill_ml.h`），供 C/C++、Android（JNI）、iOS（Swift）调用 | Preview | `cargo add rill-ml-ffi` |

### 相关项目

| 项目 | 定位 | 与 RillML 的关系 |
|---|---|---|
| [River](https://riverml.xyz/) | Python 在线学习 | RillML 受其工作流启发，独立实现，不追求兼容 |
| [Linfa](https://github.com/rust-ml/linfa) | Rust 批量学习工具集 | 侧重批量学习；RillML 侧重在线/增量学习 |
| [SmartCore](https://smartcorelib.org/) | Rust 机器学习库 | 主要面向批量学习；RillML 面向流式和边缘部署 |
| [Burn](https://burn-rs.github.io/) | Rust 深度学习框架 | 面向神经网络和 GPU；RillML 面向轻量在线模型 |

这些项目是互补的，而非竞争关系。

## 12. 稳定性

RillML 处于 1.x Stable 兼容承诺之下。以下契约在 1.x 内保持冻结：

```text
rill-ml Rust public API
selected serde model state
rill-runtime-protocol IPC v1/v2
rill-handler-api WIT ABI v1
rill-runtime public API / CLI
.rillpack 格式 v1
.rillhandler 格式 v1
stable release-index contract
```

Stable crate 的公共 API 由 `api-baseline/` 记录并由 CI 中的 `cargo-semver-checks` 强制。不要因为定位/文档升级而强行推进 2.0：只有真实 API / ABI / wire / state schema breaking change 才进入 2.0。完整政策见 [`STABILITY.md`](STABILITY.md)。

## 13. 路线图

RillML 遵循真实需求驱动的路线图。完整规划参见 [`ROADMAP.md`](ROADMAP.md)，1.0 兼容性承诺参见 [`STABILITY.md`](STABILITY.md)。

- **v0.1** — 基础闭环：预测、评估、学习、保存、恢复。
- **v0.2** — 可靠性与诊断：预测报告、冷启动、基线比较。
- **v0.3** — 稀疏特征与高维数据：FeatureHasher、FTRL、朴素贝叶斯。
- **v0.4** — 漂移检测：Page-Hinkley、ADWIN、KSWIN、自适应学习。
- **v0.5** — 在线决策：多臂老虎机、上下文老虎机。
- **v0.6** — 平台与生态：WASM、Python 绑定、Tokio Stream 适配。
- **v0.7** — 可插拔 WASM handler：签名 `.rillhandler` 包、Wasmtime 沙箱、IPC v2。
- **v0.13.0** — 最后一个 0.x 稳定版本。
- **v1.0.0-rc.1** — 1.0 候选周期开始：API、状态格式、IPC、WIT、Runtime、发布通道全部冻结。
- **v1.0.0** — 首个 1.x 正式稳定版本；不可变公开资产通过独立 host smoke 后才推进 `local-ai-stable`。
- **v1.1.0** — 可解释在线决策：延迟反馈、特征身份、漂移共识、带权学习与 Preview Runtime v3/Handler v2。
- **v1.2.0** — 发布准入与全平台复验：发布准入门禁、RISC-V/LoongArch/FreeBSD/Windows 原生复验、索引 schema v3 与 `targetLibc`。
- **v1.3.0** — 产品面一致性门禁、Stable/Preview 版本收敛、TrustMetadata v1 密钥轮换生命周期、离线/已发布 Conformance Kit、真实 fuzz harness、CycloneDX/SPDX SBOM 与 FTRL 资源诊断。
- **v1.5.0** — Stateful Runtime v3 生产资格边界、hard resource limits、结构化 health/diagnostics、状态回滚与后置 simulated consumer/fault qualification harness。

当前 1.x 不实现完整深度学习；未来方向是"冻结的神经编码器 + RillML 在线自适应头"（如 DL embedding → LinearRegression / LogisticRegression / LinUCB / Bandit → online adaptation），并在架构上预留 `inference-provider` 抽象。RillML 不应变成 PyTorch/tinygrad 的替代品。

## 14. 命名

本项目正式名称为 **RillML**。

**Rill** 是简写展示名与对话名，仅在上下文无歧义时使用。本项目不主张裸 `rill` package、module 或 CLI namespace。现有技术名称如 `rill-ml`、`rill-runtime`、`rill_ml` 有意保持显式。详见 [`docs/NAMING.md`](docs/NAMING.md)。

## 正确性与验证

RillML 通过多层验证保证正确性：

- 单元测试、集成测试与文档测试覆盖核心数学、序列化往返与边界行为；当前规模见 `cargo test --workspace` 输出与 CI summary。
- 序列化往返测试覆盖所有有状态类型。
- `proptest` 性质测试与固定随机种子（`rand_chacha`）确定性测试。
- CI 中以 `-D warnings` 强制 Clippy，rustfmt 强制执行。
- 所有示例实际运行通过。

**数值稳定性：** Welford 算法计算方差；数值稳定的 sigmoid；带 epsilon 保护的缩放；公共 API 不 panic，所有错误以 `Result<_, RillError>` 返回。

## 许可证

MIT 许可证（[LICENSE-MIT](LICENSE-MIT)）。

## 贡献

欢迎贡献。提交 Pull Request 前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。RillML 遵循"真实需求驱动"的开发原则：每个新功能都应解决真实 Rust 应用中的实际问题。
