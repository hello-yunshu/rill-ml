# RillML：Rust 原生在线机器学习库——AI IDE 完整开发提示词（核验打磨版）

> 将本提示词完整交给 AI IDE 使用。目标是尽量一次性完成一个结构清晰、可编译、可测试、可发布、可继续扩展的 Rust 在线机器学习项目首版，而不是只生成设计方案或零散代码。
>
> 名称核验日期：2026-07-12。当前未发现 GitHub 上名为 `RillML` / `rill-ml` 的同名仓库，crates.io 索引中也未发现 `rill-ml`、`rillml` 或 `rill_ml` 条目。但名称在真正创建仓库或发布 crate 前都不构成占位，执行时仍需再次核验。

---

## 一、项目名称

项目正式名称：**RillML**

建议使用：

- GitHub 仓库名：`rill-ml`
- crates.io 包名：`rill-ml`
- Rust crate 导入名：`rill_ml`
- 文档标题：`RillML`
- 不创建名为 `rill` 的 CLI，也不把 `rill` 作为公开简称；对外统一使用 `RillML` 或 `rill-ml`

名称含义：

- `Rill` 表示小溪、细流，契合持续到来的流式数据。
- 项目受到 River 在线机器学习理念启发，但不是 River 的官方 Rust 版本，也不承诺 API 或模型结果完全兼容。
- 名称避免把项目限制在电量、设备或某一种业务场景中。
- 目前已有名为 **Rill** 的独立 BI 开源产品，因此本项目不得单独使用 `Rill` 作为产品名、二进制名或 CLI 命令，也不得模仿其品牌视觉。对外统一使用完整名称 **RillML**，README 中可用一句话说明与 Rill Data 无关联。

项目一句话定位：

> RillML is a lightweight, serializable online machine learning toolkit for native Rust applications, edge devices, and continuously changing data streams.

中文定位：

> RillML 是一个面向 Rust 应用、边缘设备和持续变化数据流的轻量在线机器学习库，首版聚焦逐条预测、增量学习、渐进式评估和模型持久化，并为后续异常与漂移检测提供基础。

GitHub Description 建议：

```text
Lightweight, serializable online machine learning for Rust applications and streaming data.
```

项目关键词：

```text
rust
machine-learning
online-learning
streaming
incremental-learning
continual-learning
edge-ai
on-device-learning
progressive-validation
local-first
```

---

# 二、你的任务

请在当前目录中创建并完整实现一个 Rust 开源项目 **RillML**。

开始编码前：

1. 再次检查目标 GitHub 仓库 `hello-yunshu/rill-ml` 是否已经存在；
2. 再次检查 crates.io 是否已出现 `rill-ml` 或规范化等价名称；
3. 如果仅 GitHub 仓库未创建，不要擅自创建远程仓库，先完成本地项目；
4. 如果 crate 名已被占用，停止修改名称相关文件并在最终报告中明确说明，不要自行换名；
5. 名称检查不是商标法律意见。

不要只输出计划、伪代码、接口草图或 TODO。请直接创建项目文件、实现代码、测试、示例、文档和 CI，并在完成后实际运行检查。

如果当前目录为空，请初始化项目。

如果当前目录已经存在内容，请先检查现有文件，在不破坏有效内容的前提下完成项目；不要重复创建冲突结构。

## 2.1 执行优先级与完成策略

这是一个较大的首版任务。请按照质量优先的顺序执行，不要同时铺开所有模块后留下大量半成品。

### P0：必须先形成可运行闭环

先完成并验证：

- 错误类型和核心 trait；
- Mean、Variance、ExponentiallyWeightedMean；
- StandardScaler；
- MeanRegressor；
- SGD LinearRegression；
- MAE、MSE、RMSE；
- RegressionPipeline；
- 渐进式评估；
- Serde Snapshot；
- `online_regression.rs` 与 `progressive_validation.rs`；若回归示例包含序列化段落，必须正确设置 `required-features = ["serde"]`；
- 核心测试、README 和基础 CI。

P0 完成后必须立即运行编译、测试、Clippy 和格式检查并修复。

### P1：在 P0 全部通过后完成

继续实现：

- 其余在线统计与滚动统计；
- MinMaxScaler、Clipper；
- AdaGrad；
- HuberLoss；
- LogisticRegression；
- 分类指标；
- ClassificationPipeline；
- `sensor_stream.rs` 与 `online_classification.rs`；
- 序列化往返测试；
- Criterion benchmarks；
- 完整开源工程文件。

### P2：只做文档路线图，不在本次实现

漂移检测、异常森林、Hoeffding Tree、FTRL、稀疏特征、WASM、Python bindings 和 `no_std`。

不得为了满足清单而提交无法编译的占位代码。若执行环境或上下文确实不足以完成 P1，应保留已通过全部质量门槛的 P0，并在最终报告中逐项说明未完成内容，不能伪称全部完成。

---

完成目标：

1. 可以通过 `cargo check`。
2. 可以通过 `cargo test` 与 `cargo test --features serde`。
3. 可以通过 `cargo clippy --all-targets --features serde -- -D warnings`。
4. 可以通过 `cargo fmt --check`。
5. 示例可以运行。
6. README 足以让普通 Rust 开发者理解和使用。
7. API 具有明确边界，不把任何电池或鼠标业务逻辑写入核心库。
8. 所有在线模型都遵循“先预测、再评估、后学习”的使用方式。
9. 模型状态可选地使用 Serde 序列化。
10. 实现必须尽量使用安全 Rust；除非有明确、必要且被测试覆盖的原因，否则不要使用 `unsafe`。

---

# 三、项目要解决的问题

传统机器学习通常采用：

```text
收集完整数据集
→ 批量训练
→ 部署固定模型
→ 定期重新训练
```

RillML 面向以下流程：

```text
收到一条数据
→ 使用当前模型预测
→ 得到真实结果
→ 更新评估指标
→ 使用这一条数据更新模型
→ 继续处理下一条数据
```

目标场景包括但不限于：

- Rust 桌面应用中的本地个性化；
- Tauri 应用；
- 外设和设备状态预测；
- 电量与能耗预测；
- IoT 和传感器数据；
- 实时异常检测；
- 网络延迟、丢包和连接状态预测；
- 服务端实时指标；
- 用户行为趋势；
- 自适应练习系统；
- 边缘设备上的轻量学习；
- 不希望依赖 Python、云服务或完整重训的应用。

核心价值不是“用 Rust 复制全部机器学习生态”，而是：

- 原生嵌入 Rust 应用；
- 无 Python 运行时；
- 增量更新；
- 有界内存；
- 低运行开销；
- 状态可序列化；
- 可观察、可评估；
- 适合长期运行；
- 能适应数据规律变化。

---

# 四、项目边界

## 4.1 本次必须完成

首版需要形成一个完整、可用的小型在线学习闭环，至少包括：

### 基础抽象

- 在线回归 trait；
- 在线二分类 trait；
- 在线统计 trait；
- 在线指标 trait；
- 变换器 trait；
- 预测与学习分离；
- 模型重置能力；
- 样本计数；
- 清晰的错误类型。

### 在线统计

至少实现：

- Count；
- Sum；
- Mean；
- Variance；
- StandardDeviation；
- Min；
- Max；
- ExponentiallyWeightedMean；
- RollingMean；
- RollingVariance。

### 预处理

至少实现：

- StandardScaler；
- MinMaxScaler；
- Clipper；
- 固定维度稠密向量输入；
- 维度错误检查；
- 在线更新统计量。

### 基线回归模型

至少实现：

- MeanRegressor；
- LastValueRegressor；
- ExponentiallyWeightedMeanRegressor。

### 在线监督模型

至少实现：

- LinearRegression；
- LogisticRegression。

### 优化器

至少实现：

- SGD；
- AdaGrad。

优化器应通过 trait 或清晰抽象与模型解耦，避免把所有更新逻辑写死在模型内部。

### 损失函数

至少实现：

- SquaredError；
- HuberLoss；
- BinaryLogLoss。

### 指标

回归指标：

- MAE；
- MSE；
- RMSE；
- R²。

分类指标：

- Accuracy；
- Precision；
- Recall；
- F1；
- LogLoss。

还应实现：

- `RollingMae`；
- `RollingMse`；
- `RollingAccuracy`；
- 逐条更新；
- `value()` 查询；
- `reset()`。

首版不要实现一个声称可包装任意指标、但无法正确移除旧贡献的通用 `RollingMetric<M>`。

### 组合能力

至少实现：

- 一个预处理器加一个模型的静态 Pipeline；
- Pipeline 可以正确处理：
  - 预测时只变换；
  - 学习时更新预处理器并更新模型；
- 明确避免标签泄漏；
- 支持序列化时，Pipeline 状态也可序列化。

### 渐进式评估

实现一个 `progressive` 或 `prequential` 评估模块：

```text
predict
→ metric.update
→ learn
```

至少提供：

- 处理迭代器样本；
- 返回最终指标；
- 可选回调或逐步结果；
- 不随机打乱时序数据。

### 模型状态

在启用 `serde` feature 时：

- 核心模型和统计量可序列化；
- 提供统一的 `persistence::Snapshot<T>` 版本信封，而不是让每个模型重复维护格式版本字段；
- 支持 JSON 示例；
- 可在文档中说明 bincode 或 postcard 的使用方式，但不要把具体格式库作为核心依赖；
- 模型自身包含已见样本数量、特征维度、参数和在线统计状态；
- `Snapshot<T>` 至少包含 `format_version` 与 `model`，加载时必须校验版本。

### 示例

至少提供：

1. `online_regression.rs`
   - 这是通用库示例，不引入 Mira 仓库；
   - 模拟带概念漂移的合成特征流；
   - 预测连续目标值；
   - 比较在线线性回归与简单平均基线；
   - 演示先预测后学习；
   - 演示模型保存与恢复。

2. `sensor_stream.rs`
   - 模拟温度或振动传感器；
   - 使用滚动统计；
   - 输出异常偏差或趋势变化；
   - 首版可使用简单预测误差与 z-score，不必实现复杂异常森林。

3. `online_classification.rs`
   - 演示 LogisticRegression；
   - 每条样本到达后逐步评估 Accuracy、F1 和 LogLoss。

4. `progressive_validation.rs`
   - 清楚展示渐进式评估流程。

### 测试与质量保证

- 单元测试；
- 集成测试；
- doctest；
- 性质测试或随机对照测试；
- 与批量公式的数值对照；
- 确定性测试；
- 序列化往返测试；
- 错误输入测试；
- 边界值测试；
- Criterion 基准测试。

### 工程文件

- `README.md`
- `README.en.md`
- `LICENSE-MIT`
- `LICENSE-APACHE`
- `CHANGELOG.md`
- `CONTRIBUTING.md`
- `CODE_OF_CONDUCT.md`
- `SECURITY.md`
- `THIRD_PARTY_NOTICES.md`
- `.gitignore`
- `.editorconfig`
- `rustfmt.toml`
- `clippy.toml`，仅在确有需要时创建；
- GitHub Actions CI；
- Dependabot；
- issue templates；
- pull request template；
- crates.io 发布元数据；
- docs.rs 元数据。

---

## 4.2 本次不要做

为了保持首版可完成、可维护，不要在本次实现：

- 完整复制 River；
- 神经网络；
- GPU；
- Burn 集成；
- ONNX；
- Python bindings；
- Node.js bindings；
- 动态插件系统；
- 分布式训练；
- 联邦学习；
- 异步模型服务；
- 数据库；
- Web 服务；
- 图形界面；
- 完整时间序列框架；
- Hoeffding Tree；
- 在线随机森林；
- Half-Space Trees；
- ADWIN；
- KSWIN；
- 复杂多臂老虎机；
- `no_std` 完整支持；
- 稀疏字符串特征；
- 自动特征工程；
- 自动调参；
- DataFrame；
- 训练数据存储系统。

可以在 Roadmap 中列出这些方向，但不得为了“预留未来”而引入复杂、未使用的抽象。

---

# 五、设计原则

## 5.1 API 简洁

基础使用体验应接近：

```rust
use rill_ml::{
    metrics::{Mae, Metric},
    models::{LinearRegression, LinearRegressionConfig},
    optim::{Optimizer, SgdConfig},
    pipeline::RegressionPipeline,
    preprocessing::StandardScaler,
    OnlineRegressor,
};

let feature_count = 3;
let scaler = StandardScaler::new(feature_count)?;

let optimizer = Optimizer::sgd(
    feature_count,
    SgdConfig {
        learning_rate: 0.02,
        l2: 0.001,
        ..Default::default()
    },
)?;

let regression = LinearRegression::new(
    feature_count,
    LinearRegressionConfig {
        optimizer,
        ..Default::default()
    },
)?;

let mut model = RegressionPipeline::new(scaler, regression)?;
let mut metric = Mae::default();

let x = [0.8, 1.0, 0.6];
let y = 1.25;

let prediction = model.predict(&x)?;
metric.update(y, prediction)?;
model.learn(&x, y)?;
```

API 可以根据 Rust 类型设计适当调整，但必须保留：

- `predict` 与 `learn` 分离；
- 错误可处理；
- 输入维度明确；
- 不要求保存完整数据集。

---

## 5.2 默认面向稠密固定维度特征

首版主要支持：

```rust
&[f64]
Vec<f64>
```

内部尽量接受切片，避免不必要克隆。

模型创建时明确特征维度，例如：

```rust
LinearRegression::new(feature_count, config)
```

遇到维度不匹配时返回明确错误，不得 panic。

暂不使用 `HashMap<String, f64>` 作为核心输入。

---

## 5.3 有界内存

所有模块必须明确是否保存窗口数据。

- 非滚动统计量应为 O(1) 内存；
- 线性模型应为 O(d)；
- 滚动统计量可为 O(window_size)；
- 禁止不受控地保存全部历史样本；
- 窗口大小为 0 时返回错误。

在文档中注明时间复杂度和空间复杂度。

---

## 5.4 可序列化，但核心不强制 Serde

使用 Cargo features：

```toml
[features]
default = []
serde = ["dep:serde"]
```

首版明确依赖标准库，不创建 `std` feature，也不宣称支持 `no_std`。不要加入 `rayon` 或其他并发 feature。

在未启用 `serde` 时，核心算法必须正常编译。

不要把 JSON、bincode、postcard 等格式耦合到核心 crate；`serde_json` 只作为 dev-dependency 用于示例和测试。

---

## 5.5 数值稳定性

在线均值和方差使用稳定算法，例如 Welford 算法。

要求：

- 不使用 `sum(x²) - sum(x)²/n` 这类容易发生严重消减误差的实现；
- 对极大值、极小值和近似常量序列编写测试；
- 对 NaN 和 Infinity 制定明确策略。

建议策略：

- 默认拒绝非有限输入，返回 `RillError::NonFiniteValue`；
- 不要静默吞掉 NaN；
- 若某个统计量允许忽略 NaN，必须通过显式配置启用，首版可以不支持。

---

## 5.6 无 panic 的公共 API

公共 API 遇到以下问题应返回 `Result`：

- 特征维度错误；
- 空特征；
- 非有限输入；
- 无效窗口；
- 学习率非法；
- 正则化参数非法；
- 二分类标签不合法；
- 尚无数据时无法计算某些指标。

只有真正不可恢复的内部不变量才可使用断言。

---

## 5.7 不盲目泛型化

首版优先支持 `f64`。

不要为了同时支持所有浮点、整数、张量和稀疏类型而制造复杂泛型。

未来可在稳定后扩展，但 v0.1 需要：

- API 好读；
- 编译错误好理解；
- 文档容易写；
- 实现容易验证。

---

# 五点八、首版交付清单

最终仓库至少要包含并实现以下公开模块：

```text
error
traits
stats
preprocessing
optim
loss
models
metrics
pipeline
evaluate
persistence（模块与 Snapshot API 使用 `#[cfg(feature = "serde")]`）
```

最终公开能力矩阵：

| 能力 | P0 | P1 |
|---|---:|---:|
| Mean / Variance / EWMean | 必须 | — |
| 其余统计与滚动统计 | — | 必须 |
| StandardScaler | 必须 | — |
| MinMaxScaler / Clipper | — | 必须 |
| MeanRegressor | 必须 | — |
| LastValue / EWMeanRegressor | — | 必须 |
| SGD LinearRegression | 必须 | — |
| AdaGrad / Huber | — | 必须 |
| LogisticRegression | — | 必须 |
| 回归指标 | 必须 | — |
| 分类指标 | — | 必须 |
| RegressionPipeline | 必须 | — |
| ClassificationPipeline | — | 必须 |
| Progressive evaluation | 必须 | — |
| Snapshot 序列化 | 必须 | — |
| Examples / tests / CI | 必须 | 完善 |

---

# 六、建议的项目结构

优先使用一个 crate，不要首版拆成大量 workspace 子 crate。

建议结构：

```text
rill-ml/
├── Cargo.toml
├── README.md
├── README.en.md
├── CHANGELOG.md
├── CONTRIBUTING.md
├── CODE_OF_CONDUCT.md
├── SECURITY.md
├── THIRD_PARTY_NOTICES.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── .editorconfig
├── .gitignore
├── rustfmt.toml
├── src/
│   ├── lib.rs
│   ├── error.rs
│   ├── traits.rs
│   ├── pipeline.rs
│   ├── stats/
│   │   ├── mod.rs
│   │   ├── count.rs
│   │   ├── sum.rs
│   │   ├── mean.rs
│   │   ├── variance.rs
│   │   ├── extrema.rs
│   │   ├── ew_mean.rs
│   │   └── rolling.rs
│   ├── preprocessing/
│   │   ├── mod.rs
│   │   ├── standard_scaler.rs
│   │   ├── min_max_scaler.rs
│   │   └── clipper.rs
│   ├── optim/
│   │   ├── mod.rs
│   │   ├── sgd.rs
│   │   └── adagrad.rs
│   ├── loss/
│   │   ├── mod.rs
│   │   ├── squared.rs
│   │   ├── huber.rs
│   │   └── log_loss.rs
│   ├── models/
│   │   ├── mod.rs
│   │   ├── baseline.rs
│   │   ├── linear_regression.rs
│   │   └── logistic_regression.rs
│   ├── metrics/
│   │   ├── mod.rs
│   │   ├── regression.rs
│   │   ├── classification.rs
│   │   └── rolling.rs
│   └── evaluate/
│       ├── mod.rs
│       └── progressive.rs
├── examples/
│   ├── online_regression.rs
│   ├── sensor_stream.rs
│   ├── online_classification.rs
│   └── progressive_validation.rs
├── tests/
│   ├── stats_reference.rs
│   ├── regression_learning.rs
│   ├── classification_learning.rs
│   ├── pipeline_behavior.rs
│   ├── progressive_order.rs
│   └── serialization.rs
├── benches/
│   ├── online_stats.rs
│   └── online_models.rs
└── .github/
    ├── workflows/
    │   ├── ci.yml
    │   ├── docs.yml
    │   └── release-check.yml
    ├── dependabot.yml
    ├── ISSUE_TEMPLATE/
    │   ├── bug_report.yml
    │   ├── feature_request.yml
    │   └── config.yml
    └── pull_request_template.md
```

可以根据实现需要调整文件，但不要把所有代码塞进 `lib.rs`。

---

# 六点一、必须遵守的架构决定

以下决定已经确定，执行者不要在编码中自行推翻：

1. 单 crate，而不是 workspace 多 crate；
2. 首版只支持标准库与 `f64` 稠密切片；
3. 回归与二分类使用两个独立 Pipeline；
4. Optimizer 和 RegressionLoss 使用具体枚举，不用 trait object；
5. 预测绝不更新状态；学习阶段顺序为 `transformer.update → transform → model.learn`；
6. Serde 通过 optional feature 启用，版本控制使用统一 `Snapshot<T>`；
7. 首版只提供可正确维护的具体滚动指标，不做万能 RollingMetric；
8. 不创建 CLI、服务端、Python 绑定、WASM 或 no_std 支持。

---

# 七、核心 trait 设计要求

请先设计一套小而稳定的 trait，不要追求完全复刻 River。

可以参考以下语义：

```rust
pub trait OnlineRegressor {
    fn feature_count(&self) -> usize;
    fn samples_seen(&self) -> u64;
    fn predict(&self, features: &[f64]) -> Result<f64, RillError>;
    fn learn(&mut self, features: &[f64], target: f64) -> Result<(), RillError>;
    fn reset(&mut self);
}
```

```rust
pub trait OnlineBinaryClassifier {
    fn feature_count(&self) -> usize;
    fn samples_seen(&self) -> u64;
    fn predict_proba(&self, features: &[f64]) -> Result<f64, RillError>;

    fn predict(&self, features: &[f64]) -> Result<bool, RillError> {
        Ok(self.predict_proba(features)? >= 0.5)
    }

    fn learn(
        &mut self,
        features: &[f64],
        target: bool,
    ) -> Result<(), RillError>;

    fn reset(&mut self);
}
```

```rust
pub trait Transformer {
    fn input_dim(&self) -> usize;
    fn output_dim(&self) -> usize;

    fn transform(
        &self,
        features: &[f64],
    ) -> Result<Vec<f64>, RillError>;

    fn update(
        &mut self,
        features: &[f64],
    ) -> Result<(), RillError>;

    fn samples_seen(&self) -> u64;
    fn reset(&mut self);
}
```

Pipeline 学习语义必须固定，不允许由实现者自行选择：

1. `predict(x)`：仅使用 Transformer 当前状态执行 `transform(x)`，然后调用模型预测，任何组件都不得更新。
2. `learn(x, y)`：
   - 先调用 Transformer 的 `update(x)`，只允许使用特征 `x`，不得读取目标 `y`；
   - 再使用更新后的 Transformer 状态执行 `transform(x)`；
   - 最后使用转换后的特征调用模型 `learn`。
3. 渐进式评估器始终执行 `predict → metric.update → learn`。
4. 编写调用顺序测试和状态变化测试。

这里不把“Transformer 在学习阶段观察当前 `x`”称为标签泄漏，因为预测已经在学习前完成，而且无监督 Transformer 不接触 `y`。真正需要禁止的是：在生成当前样本的评估预测前更新任何模型或预处理状态。

指标可以使用：

```rust
pub trait Metric {
    type Truth;
    type Prediction;

    fn update(
        &mut self,
        truth: Self::Truth,
        prediction: Self::Prediction,
    ) -> Result<(), RillError>;

    fn value(&self) -> Option<f64>;
    fn samples_seen(&self) -> u64;
    fn reset(&mut self);
}
```

允许根据可组合性调整，但不要依赖动态分发完成所有功能。

---

# 八、错误类型

创建统一的 `RillError`，建议使用 `thiserror`。

至少覆盖：

```rust
pub enum RillError {
    DimensionMismatch {
        expected: usize,
        actual: usize,
    },
    EmptyFeatures,
    InvalidWindowSize,
    InvalidLearningRate(f64),
    InvalidParameter {
        name: &'static str,
        value: f64,
    },
    NonFiniteValue {
        field: &'static str,
        value: f64,
    },
    InvalidProbability(f64),
    InsufficientData,
    IncompatibleStateVersion {
        expected: u32,
        actual: u32,
    },
}
```

错误文案必须对开发者有帮助。

所有可能接收非法配置的公共构造函数统一返回 `Result<Self, RillError>`；不要一部分 panic、一部分返回 Result。

不要在正常错误路径里使用字符串拼接型“其他错误”兜底。

---

# 九、在线统计实现要求

## 9.1 Mean

使用稳定的增量更新：

```text
count += 1
delta = x - mean
mean += delta / count
```

## 9.2 Variance

使用 Welford 算法维护：

- count；
- mean；
- M2。

明确区分：

- population variance；
- sample variance。

可以通过枚举配置：

```rust
pub enum VarianceKind {
    Population,
    Sample,
}
```

样本不足时返回 `None`，不要返回误导性的 0。

## 9.3 RollingMean 和 RollingVariance

- 使用固定容量窗口；
- 达到容量后移除最旧值；
- 正确处理窗口未满；
- 不允许窗口为 0；
- 对 RollingVariance 可以使用稳定但易维护的方式；
- 如果采用删除旧值的在线公式，必须有足够测试；
- 如果窗口通常很小，也可以在窗口内重算以优先保证正确性，并在文档说明复杂度。

## 9.4 EWMean

参数使用易懂形式，例如：

```rust
ExponentiallyWeightedMean::new(alpha)
```

要求：

```text
0 < alpha <= 1
```

首个样本直接作为初始均值。

---

# 十、预处理实现要求

## 10.1 StandardScaler

每个特征维护：

- count；
- mean；
- M2。

支持配置：

```rust
StandardScalerConfig {
    with_mean: bool,
    with_std: bool,
    epsilon: f64,
}
```

行为必须统一：

- 某特征 `count == 0` 时，视为 `mean = 0`、`scale = 1`，因此返回原值；
- 某特征尚无有效方差或方差小于 `epsilon` 时，使用 `scale = 1`；
- `with_mean = true` 时减去当前历史均值；
- `with_std = true` 时除以当前历史标准差；
- `transform()` 不更新状态；
- `update()` 使用原始输入更新统计量；
- 不得让异常维度悄悄扩容；
- 方差为 0 时不得产生 NaN 或 Infinity。

## 10.2 MinMaxScaler

每个特征维护最小值和最大值。

常量特征应返回稳定值，例如 0，而不是 NaN。

## 10.3 Clipper

支持标量上下界，并检查：

```text
min <= max
```

---

# 十一、优化器与损失

## 11.1 Optimizer 设计

首版不要使用 `Box<dyn Optimizer>`，避免动态分发、克隆和 Serde 状态恢复变得复杂。

使用一个可序列化的具体枚举：

```rust
pub enum Optimizer {
    Sgd(Sgd),
    AdaGrad(AdaGrad),
}
```

由枚举统一提供参数更新方法。优化器内部状态长度固定为 `feature_count + 1`，最后一个位置用于截距；公共构造函数只接收 `feature_count`，不要要求用户手动加一。

需要支持：

- 每个权重的状态；
- 截距更新；
- 样本序号；
- 序列化；
- reset；
- 构造时验证参数维度。

SGD 配置至少包括：

```rust
SgdConfig {
    learning_rate: f64,
    l2: f64,
}
```

可选加入：

- learning rate decay；
- gradient clipping。

但不要首版同时实现十种调度器。

## 11.1.1 损失函数的具体表示

首版不要为损失函数引入 trait object。使用清晰枚举：

```rust
pub enum RegressionLoss {
    SquaredError,
    Huber { delta: f64 },
}
```

LogisticRegression 固定使用数值稳定的 BinaryLogLoss，不需要让用户替换任意分类损失。

所有损失函数提供：

- loss value；
- 对预测值的 gradient；
- 参数验证；
- 非有限输入检查。

## 11.2 AdaGrad

每个参数维护平方梯度累积。

要求：

- epsilon；
- 初始状态；
- 数值稳定；
- 可序列化。

## 11.3 LinearRegression

支持：

- SquaredError；
- HuberLoss；
- SGD；
- AdaGrad；
- 截距；
- L2 正则；
- 预测结果裁剪可由外层 Clipper 完成，不要写死业务范围。

## 11.4 LogisticRegression

- 输出概率；
- 使用数值稳定 sigmoid；
- 输入很大或很小时不得溢出；
- BinaryLogLoss 使用概率裁剪；
- 标签使用 bool 或明确的二分类类型；
- 支持 SGD 和 AdaGrad。

---

# 十二、基线模型

项目必须强调：复杂模型应与简单基线比较。

实现：

所有基线回归器必须符合 `OnlineRegressor::predict() -> Result<f64, RillError>`，因此不能有的模型返回 `Option<f64>`、有的返回 `f64`。

统一冷启动策略：

- 构造函数接收一个有限的 `initial_prediction: f64`；
- 默认构造可使用 `0.0`；
- 未见目标样本时返回 `initial_prediction`；
- 文档必须提醒用户按业务选择合理初值；
- 不在第一次渐进式评估时返回 `InsufficientData`，否则评估流无法形成第一条预测。

## LastValueRegressor

- 未见样本时返回 `initial_prediction`；
- 学习后保存最新目标。

## MeanRegressor

- 始终预测已见目标的均值；
- 未见样本时返回 `initial_prediction`；
- 作为在线回归的最低比较基线。

## ExponentiallyWeightedMeanRegressor

- 对近期数据赋予更高权重；
- 未见样本时返回 `initial_prediction`；
- 适合随时间变化的目标。

在 README 中说明：

> 只有当在线模型在渐进式评估中持续优于基线时，才应认为模型带来了实际价值。

---

# 十三、指标实现注意事项

## 回归

### MAE

```text
sum(abs(y - y_hat)) / n
```

### MSE

```text
sum((y - y_hat)^2) / n
```

### RMSE

MSE 的平方根。

### R²

在线维护需要稳定实现，明确目标均值和残差平方和的更新方式。

如果实现存在歧义，优先参考可靠数学定义并加入与离线计算对照测试。少于两个有效样本，或目标总平方和为 0 时，`value()` 返回 `None`，不要返回 NaN 或伪造的 0/1。

## 分类

### Accuracy

### Precision

### Recall

### F1

公开类型名建议使用 `F1Score`，避免 Rust 类型名过短且含义不清。

处理分母为 0 的情况。

建议返回 `None` 或按文档明确返回 0，不得产生 NaN。

### LogLoss

使用概率裁剪：

```text
epsilon <= p <= 1 - epsilon
```

## 滚动指标

首版直接实现：

- `RollingMae`；
- `RollingMse`；
- `RollingAccuracy`。

它们可以保存固定长度窗口中的单样本贡献，并在移除最旧贡献时正确维护总和。空间复杂度为 O(window_size)。

不要在 v0.1 暴露通用 `RollingMetric<M>`，因为 Precision、Recall、F1、R² 等指标不能通过简单删除一个最终指标值正确维护。后续只有在定义了可逆贡献协议后再扩展。

---

# 十四、Pipeline 设计

首版明确提供两个静态两段 Pipeline：

```rust
RegressionPipeline<T, M>
ClassificationPipeline<T, M>
```

不要尝试用一个同时覆盖回归与分类的万能 Pipeline trait。

其中：

- `T: Transformer`
- 回归模型实现 `OnlineRegressor`
- 分类模型实现 `OnlineBinaryClassifier`。

需要支持：

```rust
let mut model = RegressionPipeline::new(
    StandardScaler::new(4),
    LinearRegression::new(4, config)?,
);
```

要求：

- 输入维度在构造时验证；
- output_dim 与模型 feature_count 匹配；
- `predict()` 不改变状态；
- `learn()` 更新 transformer 和 model；
- `samples_seen()` 语义明确；
- Serde feature 下可以保存整个 Pipeline。

---

# 十五、渐进式评估

实现类似以下语义：

```rust
for sample in stream {
    let prediction = model.predict(&sample.features)?;
    metric.update(sample.target, prediction)?;
    model.learn(&sample.features, sample.target)?;
}
```

提供样本结构：

```rust
pub struct RegressionSample {
    pub features: Vec<f64>,
    pub target: f64,
}
```

分类可对应：

```rust
pub struct BinaryClassificationSample {
    pub features: Vec<f64>,
    pub target: bool,
}
```

评估函数不应：

- 预先训练；
- 打乱样本；
- 偷看未来；
- 自动复制全部数据。

可以提供逐步记录：

```rust
ProgressiveStep {
    index,
    truth,
    prediction,
    metric_value,
}
```

但避免默认保存所有步骤；可通过回调处理，维持有界内存。

---

# 十六、模型状态与兼容

在 `serde` feature 下创建统一版本信封：

```rust
pub const SNAPSHOT_FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct Snapshot<T> {
    pub format_version: u32,
    pub model: T,
}
```

提供：

- `Snapshot::new(model)`，取得模型所有权；
- 不要求实现借用型 Snapshot，避免为了序列化引用增加不必要生命周期复杂度；
- `into_model()`，并在版本不兼容时返回 `RillError::IncompatibleStateVersion`。

版本号属于 Snapshot，不要重复放进每个模型。

模型内部继续保存：

- feature_count；
- samples_seen；
- 参数；
- 优化器状态；
- 预处理状态。

README 需要说明：

- v0.x 阶段模型状态兼容不做绝对保证；
- 破坏性格式变更必须记录在 CHANGELOG；
- 加载状态前应检查版本；
- 不要把任意用户输入直接反序列化为无限内存结构。

编写序列化往返测试：

```text
训练模型
→ 序列化
→ 反序列化
→ 相同输入预测一致
→ 继续学习后行为正常
```

---

# 十七、示例的详细要求

## 17.1 online_regression.rs

模拟 300～1000 个逐步到来的回归样本。

特征可以包括：

```text
x1
x2
x3
x4
```

目标：

```text
y
```

数据生成应包含：

- 合理趋势；
- 随机噪声；
- 少量异常点；
- 某一阶段用户设置变化。

同时比较：

- MeanRegressor；
- ExponentiallyWeightedMeanRegressor；
- StandardScaler + LinearRegression。

输出：

- 最终 MAE；
- RMSE；
- 最近窗口 MAE；
- 最终若干次预测；
- 模型是否优于基线；
- 模型保存与恢复结果。

示例不得宣称模拟结果等于真实电池模型。

在注释中说明：

> 真实应用必须先完成业务层的数据清洗，例如异常值剔除、缺失值处理和不可信样本过滤。

核心库里不要实现这些业务规则。

## 17.2 sensor_stream.rs

模拟：

- 正常温度或振动；
- 缓慢漂移；
- 突然异常。

使用：

- EWMean；
- RollingMean；
- RollingVariance；
- 简单 z-score 或预测残差。

输出异常点。

首版异常演示可放在 example 层，不必创建完整 anomaly API。

## 17.3 online_classification.rs

构造流式二分类数据：

- 标签分布稍不平衡；
- 数据规律中途发生轻微变化；
- 使用 StandardScaler + LogisticRegression；
- 打印 Accuracy、Precision、Recall、F1、LogLoss。

## 17.4 progressive_validation.rs

尽量简洁，专门展示：

```text
predict
metric update
learn
```

并在代码注释解释为什么顺序不能反过来。

---

# 十八、测试策略

## 18.1 在线统计对照

对固定序列：

```text
[1, 2, 3, 4, 5]
```

验证：

- count；
- mean；
- population variance；
- sample variance；
- min/max；
- rolling window。

## 18.2 随机序列对照

使用固定随机种子生成多组数据。

将在线结果与一次性离线计算比较，使用合理浮点容差。

推荐使用：

- `approx`
- `proptest`
- `rand` 仅作为 dev-dependency。

## 18.3 模型学习能力

线性回归测试：

```text
y = 2*x1 - 0.5*x2 + 1 + noise
```

要求训练后：

- MAE 明显下降；
- 最终权重接近真实关系；
- 不要求精确相等；
- 测试应稳定，不依赖偶然随机结果。

Logistic 回归测试：

- 构造线性可分或近似可分数据；
- 验证 LogLoss 下降；
- Accuracy 达到合理阈值。

## 18.4 评估顺序测试

设计一个模型或 mock，记录调用顺序。

确保 progressive evaluator 始终：

```text
predict → metric.update → learn
```

## 18.5 Pipeline 泄漏测试

确认预测不会修改 scaler 状态。

确认学习后 scaler 状态发生变化。

## 18.6 错误测试

覆盖：

- 错误特征维度；
- 非有限值；
- 学习率为 0 或负数；
- alpha 超范围；
- 窗口为 0；
- 概率超范围；
- 常量特征；
- 无样本指标。

## 18.7 序列化测试

仅在 `serde` feature 开启时运行。

## 18.8 文档测试

README 中的核心 Rust 代码应尽量可编译。

---

# 十九、基准测试

使用 Criterion。

至少测试：

- Mean 每次 update；
- Variance 每次 update；
- StandardScaler transform；
- 8、32、128 维 LinearRegression predict；
- 8、32、128 维 LinearRegression learn；
- LogisticRegression predict/learn；
- Pipeline predict/learn；
- 序列化大小可在示例或测试中记录。

不要在 README 宣称绝对性能领先 River、Linfa 或 Python，除非有可复现基准。

README 可以写：

> RillML is designed to avoid interpreter and IPC overhead when embedded in native Rust applications. Benchmark results depend on workload and hardware.

---

# 二十、Cargo.toml 要求

建议：

```toml
[package]
name = "rill-ml"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
license = "MIT OR Apache-2.0"
description = "Lightweight, serializable online machine learning for Rust applications and streaming data."
repository = "https://github.com/hello-yunshu/rill-ml"
documentation = "https://docs.rs/rill-ml"
readme = "README.md"
keywords = [
    "machine-learning",
    "online-learning",
    "streaming",
    "incremental",
    "edge",
]
categories = [
    "algorithms",
    "science",
]
exclude = [
    ".github/",
]
```

要求：

- 使用 Rust Edition 2024；
- MSRV 设为 Rust 1.85，这是 Edition 2024 的基线版本；
- CI 中必须实际安装并测试 1.85；
- 如果所选依赖的最新版本提高了 MSRV，优先选择仍受维护且兼容 1.85 的依赖版本，不得无说明地提高项目 MSRV；
- 不使用 nightly；
- 核心依赖尽量少。

建议核心依赖：

```text
thiserror
serde（optional）
```

开发依赖：

```text
approx
proptest
rand
rand_chacha
serde_json
criterion
```

如果 `online_regression.rs` 或其他示例直接使用 `Snapshot` / `serde_json`，在 `Cargo.toml` 中为该示例声明：

```toml
[[example]]
name = "online_regression"
required-features = ["serde"]
```

或者将序列化演示拆成独立的 `serialization.rs` 示例并为其设置 `required-features = ["serde"]`。不要让默认的 `cargo test`、`cargo package` 因为示例引用 feature-gated API 而失败。

所有仅在 Serde feature 下成立的集成测试文件顶部应使用：

```rust
#![cfg(feature = "serde")]
```

如非必要，不要引入 ndarray、nalgebra、tokio、rayon、tracing、anyhow。

小型稠密在线模型使用 `Vec<f64>` 即可。

---

# 二十一、README 内容

README.md 使用中文（主文档），README.en.md 使用英文。

英文 README（README.en.md）至少包含：

1. 项目介绍；
2. 为什么在线学习；
3. 适用场景；
4. 非适用场景；
5. 安装；
6. Quick Start；
7. Progressive evaluation；
8. Regression example；
9. Classification example；
10. Serialization；
11. Baselines；
12. Current scope；
13. Roadmap；
14. Correctness and validation；
15. Relationship to River；
16. Relationship to Linfa、SmartCore 和 Burn；
17. Naming note：始终使用 RillML，说明与 Rill Data 无关联，且不提供名为 `rill` 的 CLI；
18. License；
19. Contributing。

必须明确写：

```text
RillML is inspired by the online-learning workflow popularized by River.
It is an independent Rust project and is not affiliated with or endorsed by River.
It does not currently aim for API or model compatibility.
```

中文 README 对应翻译。

不要贬低 Python 或其他项目。

准确说明：

- Python 更适合研究、数据分析和快速算法实验；
- RillML 重点解决 Rust 原生嵌入和持续运行；
- Rust 不会让同一算法天然更准确；
- 价值主要来自工程部署、状态管理和本地运行。

---

# 二十二、许可证与第三方说明

项目许可证：

```text
MIT OR Apache-2.0
```

提供两份标准许可证文件。

`THIRD_PARTY_NOTICES.md` 中说明：

- 项目受 River 的在线学习工作流启发；
- 首版代码应独立实现；
- 不直接复制 River 源代码；
- 若未来引入或改写 BSD-3-Clause 代码，必须保留相应版权和许可证说明；
- 不使用 River 名称暗示官方关系。

不要在代码头部为所有文件添加冗长版权注释。

---

# 二十三、GitHub Actions

## ci.yml

触发：

- push；
- pull_request。

矩阵至少覆盖：

- Ubuntu；
- Windows；
- macOS；
- stable Rust；
- MSRV。

步骤：

```text
cargo fmt --check
cargo check
cargo check --features serde
cargo test
cargo test --features serde
cargo clippy --all-targets --features serde -- -D warnings
```

MSRV job 至少运行：

```text
cargo +1.85 check --lib
cargo +1.85 check --lib --features serde
```

首版使用标准库，不创建虚假的 `no_std` 或 `std` feature。

## docs.yml

检查：

```text
RUSTDOCFLAGS="-D warnings" cargo doc --features serde --no-deps
```

## release-check.yml

在 tag 或手动触发时运行：

```text
cargo package
cargo package --features serde
cargo publish --dry-run --features serde
```

`cargo publish --dry-run` 可能依赖 crates.io 网络状态，失败时需区分“代码/打包错误”和“注册表不可访问或名称状态变化”。不要自动真正发布 crates.io，也不要声称 dry-run 已为名称完成预留。

---

# 二十四、代码风格

- 使用 `cargo fmt`；
- Clippy 无警告；
- 公共 API 有 rustdoc；
- rustdoc 包含错误说明和示例；
- 不使用难以理解的缩写；
- 不过度使用宏；
- 不过度使用 trait object；
- 不使用全局可变状态；
- 不使用隐式线程；
- 不在库内部打印日志；
- 示例程序可以打印；
- 错误信息保持稳定、明确；
- 所有随机示例使用固定 seed，保证输出可复现；
- 浮点比较使用容差。

---

# 二十五、版本与发布策略

初始版本：

```text
0.1.0
```

CHANGELOG 使用 Keep a Changelog 风格。

初始状态标记：

```text
Experimental but usable
```

README 明确：

- API 在 0.x 阶段可能调整；
- 核心数学实现有测试；
- 不建议在未自行验证前用于安全关键、医疗、金融或工业控制决策；
- 用户应同时维护简单基线和业务规则兜底。

---

# 二十六、Roadmap

README 中可以列出但本次不要实现：

## 0.2

- 稀疏 FeatureId；
- FTRL-Proximal；
- Page-Hinkley；
- ADWIN；
- 更完善的 Pipeline；
- Rolling metrics 扩展；
- Feature hashing；
- 模型 checkpoint 迁移工具。

## 0.3

- Online Naive Bayes；
- Hoeffding Tree；
- 在线集成；
- 预测误差异常检测；
- 基础 Bandit；
- WASM 验证。

## 后续探索

- `no_std` 子集；
- PyO3 bindings；
- WebAssembly bindings；
- 与 Python River 的对照测试工具；
- 模型格式长期兼容；
- 多输出模型；
- 异步服务包装器；
- on-device personalization；
- edge deployment examples。

---

# 二十七、Mira 集成边界

RillML 是独立通用项目。

Mira 未来可以这样使用：

```text
Mira 业务层
├── 读取鼠标电量
├── 识别充电状态
├── 排除重连修正
├── 形成有效耗电区间
├── 构造特征
├── 调用 RillML 预测
├── 调用 RillML 学习
└── 展示续航、误差和可信度

RillML
├── 在线统计
├── StandardScaler
├── 在线线性回归
├── 指标
├── 渐进式评估
└── 序列化
```

RillML 不应包含：

- `Battery`；
- `Mouse`；
- `ChargingState`；
- `PollingRate`；
- `RGB`；
- Mira 插件协议；
- USB/HID；
- Tauri 命令；
- SQLite 表结构。

`online_regression.rs` 只是通用示例。

---

# 二十八、实施顺序

请按以下顺序实现，过程中持续运行测试，不要最后才一起修复：

1. 初始化 Cargo 项目和基础工程文件；
2. 实现错误类型；
3. 实现基础 traits；
4. 实现在线统计；
5. 编写在线统计测试；
6. 实现预处理；
7. 实现基线模型；
8. 实现损失函数；
9. 实现优化器；
10. 实现 LinearRegression；
11. 实现 LogisticRegression；
12. 实现指标；
13. 实现 Pipeline；
14. 实现 progressive evaluator；
15. 实现 Serde feature；
16. 实现示例；
17. 实现 Criterion benchmark；
18. 编写 README 和中文 README；
19. 配置 GitHub Actions；
20. 运行全部检查；
21. 修复所有 warning、测试失败和文档问题；
22. 总结最终实现。

不要为了保持“步骤完整”而留下半成品模块。

某个模块如果无法在保持正确性的前提下一次完成，宁可缩小接口，也不要提交错误实现。

---

# 二十九、验收标准

完成后必须满足：

## 编译

```bash
cargo check
cargo check --features serde
```

## 格式

```bash
cargo fmt --check
```

## 测试

```bash
cargo test
cargo test --features serde
```

## Clippy

```bash
cargo clippy --all-targets --features serde -- -D warnings
```

## 文档

```bash
RUSTDOCFLAGS="-D warnings" cargo doc --features serde --no-deps
```

## 打包

```bash
cargo package
```

## 示例

至少实际运行：

```bash
cargo run --example online_regression --features serde
cargo run --example sensor_stream
cargo run --example online_classification
cargo run --example progressive_validation
```

如果当前执行环境无法完成某一命令，必须明确说明原因，不要声称已经通过。

---

# 三十、最终回复格式

完成开发后，请先执行一次最终一致性审计：

- 搜索是否仍存在 `all-features`、虚假 `no_std`、未实现模块或旧 API 名称；
- 检查 README 示例、rustdoc 和实际公开 API 是否一致；
- 检查 Cargo features、CI 命令和验收命令是否一致，并确认没有文档要求 `--all-features` 或 `--no-default-features`；
- 检查所有示例是否使用同一套 Pipeline 学习顺序：预测时不更新，学习时 `transformer.update → transform → model.learn`；
- 检查 crate 名、导入名和完整品牌名是否一致；
- 检查不存在名为 `rill` 的二进制目标。

然后用简洁但具体的方式报告：

1. 创建了哪些核心模块；
2. 公开 API 的主要入口；
3. 已实现哪些模型、指标和统计量；
4. 哪些示例可以运行；
5. 执行了哪些命令；
6. 测试结果；
7. 是否存在未完成或妥协项；
8. 后续最合理的第一项扩展。

不要只说“项目已完成”。

---

# 三十一、重要约束汇总

开发过程中始终遵守：

- 实际创建并实现项目，不只写计划；
- 不完整复制 River；
- 不引入 Python 运行时；
- 不做电量专用库；
- 不把 Mira 业务规则写进核心；
- 不使用 nightly；
- 不虚假支持 no_std；
- 不创建名为 `rill` 的 CLI 或二进制；
- 不默认保存完整历史；
- 不静默接受 NaN；
- 不在公共错误路径 panic；
- 不过度抽象；
- 不过度拆 crate；
- 不追求算法数量；
- 优先正确性、测试和可维护性；
- 保证先预测、再评估、后学习；
- 所有随机测试和示例尽量可复现；
- 未经基准验证，不宣称性能优于其他框架；
- 未经真实数据验证，不宣称模型比简单基线更准确；
- 将 RillML 做成一个可独立使用、可继续扩展、可被 Mira 实际采用的 Rust 在线学习库。

---

# 三十二、预期的最小使用体验

最终项目至少应能支持类似代码：

```rust
use rill_ml::{
    metrics::{Mae, Metric},
    models::{LinearRegression, LinearRegressionConfig},
    optim::{Optimizer, SgdConfig},
    pipeline::RegressionPipeline,
    preprocessing::StandardScaler,
    OnlineRegressor,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let feature_count = 3;

    let scaler = StandardScaler::new(feature_count)?;

    let optimizer = Optimizer::sgd(
        feature_count,
        SgdConfig {
            learning_rate: 0.02,
            l2: 0.001,
            ..Default::default()
        },
    )?;

    let regression = LinearRegression::new(
        feature_count,
        LinearRegressionConfig {
            optimizer,
            ..Default::default()
        },
    )?;

    let mut model = RegressionPipeline::new(scaler, regression)?;
    let mut mae = Mae::default();

    let samples = [
        ([0.80, 1.00, 0.60], 1.24),
        ([0.65, 0.50, 0.00], 0.71),
        ([0.92, 1.00, 0.90], 1.58),
    ];

    for (features, target) in samples {
        let prediction = model.predict(&features)?;
        mae.update(target, prediction)?;
        model.learn(&features, target)?;
    }

    println!("MAE: {:?}", mae.value());

    Ok(())
}
```

实际 API 可以为了类型清晰和编译正确做适当调整，但整体体验不得显著复杂于此。

---

请现在开始检查当前目录并执行完整实现。


---

# 三十三、名称最终约束

本提示词采用 **RillML** 作为正式名称，原因是精确名称具有辨识度，并且在核验时未发现同名 GitHub 仓库或 crates.io 条目。

但执行者必须理解：

- 公开注册表名称会随时变化；
- 本次核验不等于名称预留；
- `Rill` 本身已被其他成熟软件使用；
- 本项目对外必须写作 `RillML`；
- 仓库与 crate 使用 `rill-ml`；
- Rust 导入名为 `rill_ml`；
- 不创建 `rill` CLI；
- 不宣称与 River、Rill Data 或其他同名项目存在官方关系；
- 正式发布前应进行独立商标与域名核验。

- 已额外核验 `MiraML` / `mira-ml`：GitHub 已存在 `Mira-ML` 组织、`mira-ml` 仓库和 `miraml` 仓库，并有团队实际使用 `mira.ml` 域名。因此本项目不要改名为 MiraML。
