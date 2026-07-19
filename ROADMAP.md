# RillML 长期发展规划

> 项目定位：面向 Rust 应用、边缘设备和持续数据流的轻量在线机器学习库。  
> 本规划强调“真实需求驱动、可靠性优先、逐步扩展”，不以完整复制 River 为目标。

---

## 一、项目长期目标

RillML 的目标不是成为算法数量最多的 Rust 机器学习框架，而是成为：

> 一个可直接嵌入 Rust 应用、能够持续学习、支持本地运行、状态可持久化、结果可诊断、适合真实产品使用的在线学习工具箱。

长期价值主要体现在：

- 不依赖 Python 运行时；
- 可直接嵌入桌面应用、服务端、边缘设备和 IoT；
- 支持逐条预测、逐条学习；
- 数据规律变化后能够持续适应；
- 能够评估自身是否优于简单基线；
- 模型状态可保存、恢复和迁移；
- 对隐私敏感场景友好；
- 可作为 Mira 等真实产品的底层智能能力。

---

# 二、核心发展原则

## 1. 真实问题驱动

每个新功能都应从真实应用问题出发，而不是因为其他框架中已有同类模块。

推荐流程：

```text
真实应用暴露问题
→ 验证现有简单方案是否不足
→ 设计最小通用解法
→ 在真实项目中验证
→ 再加入 RillML
```

不推荐：

```text
看到 River 有某模块
→ 直接照着增加
```

---

## 2. 可靠性优先于算法数量

每个进入核心库的能力至少需要：

- 明确数学定义；
- 边界条件说明；
- 单元测试；
- 随机对照测试；
- 与离线计算或可靠实现对照；
- 时间复杂度；
- 空间复杂度；
- 真实示例；
- 清晰的使用限制。

宁可保留 10 个可靠模块，也不要快速扩展成 50 个难以验证的模块。

---

## 3. 简单模型必须保留

复杂模型不能替代基线。

任何真实应用都应至少比较：

```text
LastValue
Mean
ExponentiallyWeightedMean
OnlineLinearRegression
```

只有当复杂模型在渐进式评估中长期优于基线时，才应启用复杂模型输出。

---

## 4. 默认保持有界内存

RillML 的核心特征之一是适合持续运行。

因此：

- 不默认保存全部历史样本；
- 在线统计量优先使用 O(1) 空间；
- 线性模型优先使用 O(d) 空间；
- 滚动窗口必须明确最大长度；
- 稀疏模型必须控制动态特征增长；
- 调试摘要与原始训练数据分离。

---

## 5. 核心 API 稳定优先

公共接口应尽量保持精简：

```rust
predict
learn
reset
samples_seen
feature_count
```

避免过早引入：

- 复杂关联类型；
- 大量 trait object；
- 任意动态 Pipeline；
- 多层泛型组合；
- 自动特征推断；
- 隐式线程或异步。

---

## 6. Rust 负责生产，Python 可负责实验

长期可以形成：

```text
Rust 核心
├── 在线统计
├── 在线模型
├── 持久化
├── 渐进式评估
└── 漂移检测

Python 绑定
├── Jupyter 实验
├── 数据分析
├── 与 River 对照
├── 可视化
└── 快速验证
```

但 Python 绑定是后期生态能力，不影响 Rust API 的一等地位。

---

# 三、版本规划总览

| 版本 | 主题 | 核心目标 |
|---|---|---|
| v0.1 | 基础闭环 | 能被 Mira 等真实应用接入 |
| v0.2 | 可靠性与诊断 | 不只预测，还能判断是否可信 |
| v0.3 | 高维与稀疏数据 | 扩展到用户行为和服务端事件 |
| v0.4 | 漂移与自适应 | 数据规律改变后仍能工作 |
| v0.5 | 在线决策 | 从预测扩展到策略选择 |
| v0.6 | 生态与平台 | 扩大接入范围 |
| v0.7 | 可插拔 WASM handler | runtime 加载签名 WASM handler、IPC v2、统一发布流水线 |
| v1.0 | 稳定版本 | API、状态格式和生产能力稳定 |

---

# 四、v0.1：基础闭环

> 状态：已完成（v0.1.0，2026-07-12）

## 目标

形成一个完整、可用、可验证的在线学习基础库。

核心问题：

> RillML 能否在 Rust 应用中完成“预测—评估—学习—保存—恢复”的完整闭环？

---

## 必须完成

### 在线统计

- Count
- Sum
- Mean
- Variance
- StandardDeviation
- Min
- Max
- ExponentiallyWeightedMean
- RollingMean
- RollingVariance

### 预处理

- StandardScaler
- MinMaxScaler
- Clipper

### 基线模型

- LastValueRegressor
- MeanRegressor
- ExponentiallyWeightedMeanRegressor

### 在线模型

- LinearRegression
- LogisticRegression

### 优化器

- SGD
- AdaGrad

### 损失函数

- SquaredError
- HuberLoss
- BinaryLogLoss

### 指标

回归：

- MAE
- MSE
- RMSE
- R²
- RollingMAE
- RollingMSE

分类：

- Accuracy
- Precision
- Recall
- F1Score
- LogLoss
- RollingAccuracy

### Pipeline

- RegressionPipeline
- ClassificationPipeline

### 渐进式评估

统一顺序：

```text
predict
→ metric.update
→ learn
```

### 状态保存

- Serde optional feature
- Snapshot<T>
- 格式版本
- 模型状态恢复
- 序列化往返测试

### 示例

- 在线回归预测
- 传感器数据流
- 在线二分类
- 渐进式评估

---

## 验收标准

- Mira 可以直接依赖 crate；
- 不需要 Python；
- 默认内存有界；
- 模型可以保存和恢复；
- 回归与分类都有完整示例；
- 数值实现通过随机对照测试；
- CI 覆盖 Linux、macOS 和 Windows；
- 所有公共 API 有文档；
- 不宣称支持 no_std；
- 不实现动态万能 Pipeline。

---

## 不做

- 漂移检测；
- FTRL；
- 稀疏字符串特征；
- Hoeffding Tree；
- Python bindings；
- WASM；
- 多臂老虎机；
- 自动调参。

---

# 五、v0.2：可靠性、可信度与诊断

> 状态：已完成（v0.2.0，2026-07-13）

## 目标

让上层应用不仅得到预测结果，还能判断：

- 模型是否已经学够；
- 最近误差如何；
- 是否优于基线；
- 是否正在冷启动；
- 预测是否稳定；
- 模型状态是否异常。

---

## 1. PredictionReport

增加可选的诊断包装层：

```rust
PredictionReport {
    prediction,
    lower_bound,
    upper_bound,
    confidence,
    samples_seen,
    recent_error,
    baseline_error,
    is_warming_up,
}
```

核心模型仍可只返回简单预测，诊断能力放在包装器中。

---

## 2. 冷启动策略

支持明确的冷启动状态：

```text
NoData
WarmingUp
Usable
Stable
Degraded
```

可根据以下信息判断：

- 有效样本数；
- 最近误差；
- 特征覆盖；
- 与基线比较；
- 漂移状态。

不应仅根据固定样本数机械判断。

---

## 3. 基线比较器

增加模型比较能力：

```text
MeanRegressor
EWMeanRegressor
LinearRegression
```

持续记录每个模型的渐进式指标。

输出：

- 当前最佳模型；
- 每个模型最近误差；
- 是否切换；
- 切换原因。

---

## 4. 模型选择器

增加基础 `OnlineModelSelector`：

```rust
selector.predict(&x)
selector.learn(&x, y)
```

首版策略可包括：

- 最近窗口最小 MAE；
- 指数衰减误差；
- 切换冷却期；
- 防止频繁抖动。

---

## 5. 预测区间

第一阶段使用简单残差区间：

```text
prediction ± k × recent_error
```

后续探索：

- 在线残差分位数；
- Quantile sketch；
- Conformal prediction；
- 分位数回归。

---

## 6. 模型诊断

提供：

```rust
model.inspect()
model.validate_state()
```

检查内容：

- 样本数量；
- 特征维度；
- 权重范围；
- 是否出现 NaN / Infinity；
- 优化器状态；
- Scaler 状态；
- 模型状态大小；
- 最近错误统计。

---

## 7. 训练摘要

不保存完整样本，只维护摘要：

- 总样本数；
- 被拒绝输入数；
- 最近误差；
- 历史最佳误差；
- 基线误差；
- 模型切换次数；
- 重置次数；
- 状态加载失败次数。

---

## 验收标准

- 上层应用可以展示“低/中/高”可信度；
- 模型可证明是否优于基线；
- 冷启动期间不会输出虚假精度；
- PredictionReport 不污染基础模型 API；
- 所有诊断信息仍保持有界内存。

---

# 六、v0.3：稀疏特征与高维数据

> 状态：已完成（v0.3.0，2026-07-13）

## 目标

让 RillML 从低维设备数据扩展到：

- 用户行为；
- 实时推荐；
- 点击预测；
- 日志分类；
- 服务端事件；
- 高维分类特征。

---

## 1. SparseFeatures

增加稀疏输入：

```rust
SparseFeatures {
    values: Vec<(FeatureId, f64)>
}
```

要求：

- FeatureId 使用整数；
- 特征排序规则明确；
- 重复 FeatureId 行为明确；
- 支持零值省略；
- 不使用 `HashMap<String, f64>` 作为核心表示。

---

## 2. FeatureHasher

提供固定维度特征哈希：

```rust
FeatureHasher::new(dimension, seed)
```

要求：

- 哈希结果可复现；
- 支持 signed hashing；
- 明确冲突不可避免；
- 维度固定；
- 支持字符串特征到 FeatureId。

---

## 3. 分类特征编码

逐步加入：

- OneHotEncoder；
- OrdinalEncoder；
- FrequencyEncoder；
- MissingIndicator。

目标编码推迟到后期，因为容易产生标签泄漏。

---

## 4. 缺失值处理

增加：

- ConstantImputer；
- MeanImputer；
- ForwardFill；
- MissingIndicator。

明确区分：

- 缺失；
- 0；
- 未出现稀疏特征。

---

## 5. FTRL-Proximal

加入适合高维稀疏场景的 FTRL。

支持：

- L1；
- L2；
- 稀疏参数；
- 逻辑回归；
- 动态特征；
- 状态序列化。

---

## 6. Online Naive Bayes

优先考虑：

- Gaussian Naive Bayes；
- Bernoulli Naive Bayes；
- Multinomial Naive Bayes。

用于：

- 文本；
- 事件分类；
- 低成本分类；
- 小样本在线学习。

---

## 验收标准

- 稀疏模型不需要预先知道所有特征；
- FeatureHasher 输出可复现；
- FTRL 在高维数据上优于稠密线性模型；
- 动态特征增长有内存策略；
- 示例至少覆盖点击或事件分类。

---

# 七、v0.4：漂移检测与自适应

> 状态：已完成（v0.4.0，2026-07-13）

## 目标

解决现实中的规律变化：

- 电池老化；
- 用户习惯改变；
- 固件更新；
- 网络状态变化；
- 传感器漂移；
- 服务负载模式改变。

---

## 1. Page-Hinkley

优先实现，适合检测平均值持续变化。

要求：

- 明确参数含义；
- 提供触发测试；
- 提供正常序列误报测试；
- 可用于目标值或预测误差流。

---

## 2. ADWIN

用于自适应窗口变化检测。

要求：

- 按论文实现；
- 内存复杂度明确；
- 与可靠参考实现对照；
- 支持漂移和警告状态。

---

## 3. KSWIN

通过窗口分布比较识别变化。

适合：

- 非均值型变化；
- 分布形态变化；
- 连续数值流。

---

## 4. 漂移策略

提供：

```rust
DriftAction::NotifyOnly
DriftAction::ReduceConfidence
DriftAction::ResetModel
DriftAction::ResetPreprocessor
DriftAction::ReplaceWithBaseline
DriftAction::IncreaseAdaptationRate
```

检测器和处理策略应解耦。

---

## 5. 衰减学习

支持：

- 指数衰减统计；
- 时间衰减样本权重；
- 动态学习率；
- 固定窗口训练；
- 近期数据优先。

---

## 6. DriftAwareModel

增加包装器：

```rust
DriftAwareModel<M, D, A>
```

负责：

- 预测；
- 评估误差；
- 将误差输入漂移检测器；
- 触发处理策略；
- 记录漂移事件。

避免把漂移逻辑写进每个模型。

---

## 验收标准

- 能检测人工构造的规律变化；
- 正常数据误报率可接受；
- 检测器本身内存有界；
- 检测与处理策略解耦；
- 模型发生漂移后可回退到基线；
- 不默认自动清空模型。

---

# 八、v0.5：从预测扩展到决策

> 状态：已完成（v0.5.0，2026-07-13）

## 目标

让 RillML 不仅回答：

```text
未来会发生什么？
```

还可以回答：

```text
当前应该选择哪个策略？
```

---

## 1. Epsilon-Greedy

适合作为最简单的在线策略选择算法。

支持：

- 固定 epsilon；
- 衰减 epsilon；
- 每个 arm 的奖励统计。

---

## 2. UCB1

用于平衡探索与利用。

要求：

- 未探索 arm 的处理明确；
- 奖励范围要求明确；
- 数值稳定。

---

## 3. Thompson Sampling

先支持 Bernoulli reward：

- Beta 分布；
- 成功 / 失败；
- 状态持久化。

---

## 4. Contextual Bandit

在基础 Bandit 稳定后探索：

- 当前上下文特征；
- 每个策略对应模型；
- LinUCB；
- Logistic Thompson Sampling。

---

## 应用场景

- 网络节点选择；
- 缓存策略；
- 通知时机；
- 功能入口排序；
- 学习题目推荐；
- 设备省电策略；
- 用户界面个性化。

---

## 验收标准

- Bandit 独立于监督学习模型；
- 奖励定义由业务层负责；
- 示例包含探索与利用；
- 不将 Bandit 包装成无条件自动决策；
- 提供安全回退策略。

---

# 九、v0.6：平台与生态扩展

> 状态：已完成（v0.6.0，2026-07-15）

## 目标

扩大 RillML 的接入范围，同时保持 Rust 核心稳定。

---

## 1. WASM

支持：

- 浏览器本地学习；
- 隐私优先个性化；
- 离线网页；
- 教育类应用；
- 前端预测。

要求：

- 二进制体积可接受；
- 不依赖系统线程；
- 不依赖文件系统；
- 使用可移植随机数来源。

---

## 2. Python bindings

通过 PyO3 / Maturin 提供：

- 数据分析；
- Jupyter；
- 与 River 对照；
- 快速原型；
- 可视化实验。

原则：

- Rust 是唯一核心实现；
- Python 不维护第二套算法；
- Python API 可以更符合 Python 习惯；
- 不要求与 River 完全兼容。

---

## 3. Tokio Stream 适配

以独立可选 crate 或 feature 提供：

```text
tokio::Stream
→ progressive evaluation
```

核心模型保持同步。

---

## 4. Polars / Arrow 适配

提供数据转换辅助，不把 DataFrame 引入核心 crate。

可拆分为：

```text
rill-ml-polars
rill-ml-arrow
```

---

## 5. CLI 工具

仍不创建名为 `rill` 的 CLI。

若未来确有模型检查需求，可考虑：

```text
rillml-inspect
```

仅用于：

- 查看 Snapshot；
- 检查版本；
- 输出模型摘要；
- 执行迁移。

不是核心运行依赖。

---

## 验收标准

- Rust API 不因绑定层变得复杂；
- 各平台绑定可独立发布；
- 核心 crate 仍保持轻量；
- 外部适配器不强制成为默认依赖。

---

# 十、v0.7：可插拔 WASM handler

> 状态：当前（v0.8.1，2026-07-19）

## 目标

把 `rill-runtime` 从绑定具体业务的运行时升级为业务中立的通用推理运行时：runtime 加载经过签名验证的 WASM handler 组件，handler 实现具体 capability；更新 handler 不需要重新编译或替换 `rill-runtime` 二进制。Runtime、模型包与 handler 包可各自独立更新。

核心问题：

> 能否让宿主应用只依赖协议 crate，让 runtime、模型和 handler 三者独立演进、独立发布、独立回滚？

---

## 1. 可插拔 WASM handler 架构

- 新增 `rill-handler-api` crate，定义版本化 WIT handler 契约（`invoke-handler` world），导出 `HANDLER_API_VERSION = 1`。Handler 作者依赖该 crate 获得规范 ABI，runtime 使用其 host 侧绑定。
- 新增 handler 包格式 `.rillhandler`：签名 ZIP 包，包含 `manifest.json`、`handler.wasm`、`checksums.json` 与 `META-INF/signature.ed25519`。Manifest 声明 handler id、版本、handler API 版本、最低 runtime 版本、capabilities 与模块 SHA-256。
- handler 与 model 使用独立 trust store，模型密钥不能签署 handler，反之亦然。
- `rill-runtime::handler` 模块提供共享类型（`HandlerIdentity`、`HandlerLoadError`）、`effective_capabilities()`（模型与 handler 能力交集）、内置 `LinearRegressionInvokeHandler`（从 `server.rs` 迁出）与 `WasmInvokeHandler`（`wasm` feature 后）。
- CLI 选项：`rill-runtime serve` 接受 `--handler <path.rillhandler>`、`--handler-trust-key KEY=HEX`、`--builtin-handler linear-regression`；`--handler` 与 `--builtin-handler` 互斥。未指定 handler 时回退到内置线性回归并打印弃用提示。
- `rill-pack` 新增 `create-handler` 与 `inspect-handler` 子命令。

---

## 2. Wasmtime 46 沙箱

- `WasmInvokeHandler` 在 Wasmtime 沙箱内执行 handler：无 WASI 权限（不访问文件系统、网络、环境变量、stdio 和进程），每次调用有独立 fuel 预算和 epoch 超时，内存上限 64 MiB，table 上限 10 000 条目，I/O JSON 上限 1 MiB。
- 实例化前校验 guest `metadata()` 与签名 manifest 一致；trap、超时映射为稳定错误码。
- v0.7.1 将 wasmtime 从 27 升级到 46.0.1，修复包括 Critical（CVSS 9.0）aarch64 沙箱逃逸（CVE-2026-34971，RUSTSEC-2026-0096）在内的 15 个未修补安全公告。wasmtime 27 不在受支持发布线，wasmtime 46 是当前稳定发布线，已知公告全部解决。
- 因 wasmtime 46 要求，工作区 MSRV 由 1.85 升至 1.94。CI MSRV 检查、README、CONTRIBUTING、HANDLER-RFC、THIRD_PARTY_NOTICES 同步更新。
- handler host 代码与 wasmtime 27 源码兼容，无 API 改动。

---

## 3. IPC v1/v2 共存

- `RUNTIME_API_VERSION` 升至 2。V2 握手增加 `handlerId`、`handlerVersion`、`handlerApiVersion` 与 `effectiveCapabilities`。
- V1 响应完全省略 handler 字段；V2 响应包含完整 handler 身份。两个 wire schema 是独立类型，不使用带大量 `Option` 字段的结构冒充两个版本。
- 内部 `EngineResponse` 捕获所有响应数据（含 handler 身份），在 IPC 边界转换为 `RuntimeResponse`（v1）或 `RuntimeResponseV2`（v2）。
- Runtime 根据请求的 `apiVersion` 选择响应格式，同时服务 v1 与 v2 客户端。

---

## 4. 统一发布流水线

- v0.7.1 将 `ci.yml` 与 `release.yml` 合并为单一 `pipeline.yml`：CI 在 push/PR 运行，发布在 `workflow_dispatch`（由 Auto Release 在 `vX.Y.Z` 标签上触发，CI 通过后派发）运行。故意省略 tag-push 触发以避免重复运行。
- 发布索引 schema 升至 v2，`RELEASE_INDEX_SCHEMA_VERSION = 2`。`ReleaseArtifactKind` 新增 `Handler` 变体（平台无关，需 `handlerApiVersion` 与 `minRuntimeVersion`，无 OS/arch 字段）。`build-release-index.py` 支持 `--handler-id`、`--handler-version`、`--handler-min-runtime`。
- v0.7.2 起停止发布 Intel macOS Runtime 二进制，官方 macOS 发布与签名稳定索引仅含 Apple Silicon（ARM64）；Linux 与 Windows 仍为 x86_64。
- 工作流允许部分发布失败后安全重跑：已发布 crate 跳过，已存在 Release 复用不可变资产，并继续修复 `local-ai-stable` 索引指针。已发布版本标签不得移动或覆盖。
- v0.7.1 修复 v0.7.0 发布时 stable-index schema 不兼容导致 `verify-index` 失败的问题（legacy v1 schema 失败改为告警而非失败）；`rill-pack create-handler` 自动从 WASM 模块字节计算 `moduleSha256` 与 `moduleSize`。

---

## 验收标准

- 更新 handler 不需要重新编译或替换 `rill-runtime` 二进制；
- handler trap、超时或非法输出后 runtime 进程仍能返回 health/error 响应；
- v1 与 v2 客户端均可正常握手，v1 响应不含 handler 字段；
- 发布索引签名覆盖每个二进制、模型包与 handler 包的 SHA-256、大小、版本、平台与 URL；
- macOS Runtime 除发布索引签名外还通过 `codesign --verify --strict`；
- Runtime、模型、handler 三者更新彼此独立，但都必须通过启动自检后才能切换为 `current`。

---

# 十一、v1.0：稳定版本条件

不要只因为功能多就发布 1.0。

建议至少满足：

## API 稳定

- 核心 trait 至少经过两个真实项目验证；
- 不再频繁改动 predict / learn 语义；
- Pipeline 行为稳定；
- 错误类型稳定。

## 状态兼容

- Snapshot 有正式版本策略；
- 至少支持一个旧版本迁移；
- 加载失败行为明确；
- 状态校验完善。

## 正确性

- 核心模型都有参考对照；
- 漂移检测有可重复测试；
- 随机测试长期稳定；
- 无已知严重数值问题。

## 真实使用

至少满足其一：

- 两个独立生产项目使用；
- Mira 外还有一个真实使用者；
- crate 已形成稳定外部用户反馈。

## 文档

- 英文和中文 README；
- API 文档；
- 模型使用指南；
- 迁移指南；
- 性能说明；
- 安全限制；
- 真实案例。

---

# 十二、Mira 作为首个真实验证项目

Mira 不只是一个示例，而是 RillML 的首个生产验证场景。

---

## Mira 业务层负责

- 读取设备电量；
- 识别充电状态；
- 处理重连；
- 排除错误跳变；
- 形成有效耗电区间；
- 计算活跃时间；
- 构造回报率、灯光、连接方式等特征；
- 展示预测和可信度；
- 决定是否回退到规则模型。

---

## RillML 负责

- 在线标准化；
- 基线模型；
- 在线线性回归；
- 渐进式指标；
- 模型比较；
- 预测区间；
- 漂移检测；
- 模型状态保存；
- 状态恢复和诊断。

---

## Mira 不应反向污染核心库

RillML 不出现：

```text
Mouse
Battery
Charging
PollingRate
RGB
Receiver
HID
Tauri
MiraPlugin
```

这些只能存在于 Mira 集成层或示例中。

---

# 十三、官方真实案例规划

## 案例一：Mira 电量预测

验证：

- 数据清洗；
- 冷启动；
- 在线回归；
- 基线比较；
- 状态恢复；
- 可信度；
- 电池老化漂移。

---

## 案例二：网络延迟预测

输入：

- 节点；
- 时间段；
- 延迟；
- 丢包；
- 连接失败；
- 协议类型。

验证：

- EWMean；
- LinearRegression；
- 漂移检测；
- 异常提醒；
- 模型选择。

---

## 案例三：传感器异常检测

输入：

- 温度；
- 振动；
- 湿度；
- 电流；
- 转速。

验证：

- Rolling statistics；
- Page-Hinkley；
- ADWIN；
- 状态持久化；
- 边缘设备长期运行。

---

## 案例四：用户行为分类

输入：

- 时间；
- 功能使用；
- 最近操作；
- 用户状态；
- 稀疏分类特征。

验证：

- FeatureHasher；
- FTRL；
- LogisticRegression；
- 稀疏特征；
- 在线分类指标。

---

# 十四、每次新增功能前的决策模板

每个候选功能必须回答以下问题。

## 1. 真实需求

```text
哪个真实项目需要它？
当前问题是什么？
```

## 2. 简单方案

```text
现有基线是否已经足够？
为什么不够？
```

## 3. 通用价值

```text
这是 Mira 专用需求，还是多个项目都能复用？
```

## 4. 工程负担

```text
会增加多少公共 API？
会增加哪些序列化状态？
会增加哪些组合测试？
```

## 5. 数学验证

```text
参考公式或论文是什么？
如何与离线实现对照？
```

## 6. 性能

```text
时间复杂度？
空间复杂度？
是否仍然有界内存？
```

## 7. 兼容性

```text
是否影响旧 Snapshot？
是否需要迁移？
```

## 8. 验收标准

```text
怎样证明它确实比现有方案更好？
```

答不清楚时，不加入核心库。

---

# 十五、优先级评估方法

候选功能可以按以下维度评分，每项 1～5 分：

| 维度 | 说明 |
|---|---|
| 真实需求 | 是否有明确项目需要 |
| 用户价值 | 能否明显改善结果或体验 |
| 通用程度 | 是否适用于多个场景 |
| 实现成本 | 分数越高表示成本越低 |
| 维护成本 | 分数越高表示维护越轻 |
| 可验证性 | 是否容易证明正确 |
| API 风险 | 分数越高表示破坏风险越低 |

建议优先开发：

```text
总分高
+
至少一个真实使用者
+
可明确验收
```

而不是看起来最先进的算法。

---

# 十六、维护边界

## 单人可长期维护的推荐范围

```text
在线统计
预处理
线性模型
逻辑回归
FTRL
朴素贝叶斯
指标
Pipeline
基线比较
预测区间
简单漂移检测
状态持久化
基础 Bandit
```

---

## 容易使项目变成大型框架的范围

```text
在线随机森林
复杂树集成
完整时间序列
神经网络
分布式训练
联邦学习
多语言绑定矩阵
GPU
自动调参
DataFrame
模型服务平台
```

这些方向只有在形成社区和维护团队后再考虑。

---

# 十七、长期技术债务清单

需要持续关注：

## API 技术债

- trait 是否过于复杂；
- Builder 是否重复；
- Pipeline 类型是否膨胀；
- 错误类型是否含糊；
- 命名是否一致。

## 数值技术债

- 极端值；
- NaN / Infinity；
- 长序列累计误差；
- 方差接近 0；
- sigmoid 溢出；
- 学习率过大；
- 参数发散。

## 状态技术债

- Snapshot 字段变化；
- 优化器状态迁移；
- Scaler 状态迁移；
- 旧版本兼容；
- 损坏文件恢复。

## 文档技术债

- README 示例与 API 不一致；
- 旧名称残留；
- feature 说明过时；
- Roadmap 与实际状态不一致；
- 性能结论缺少可复现基准。

## 测试技术债

- 测试仅覆盖固定样本；
- 随机测试不稳定；
- 缺少长序列测试；
- 缺少恢复后继续学习测试；
- 漂移测试误报未覆盖。

---

# 十八、季度维护建议

如果由个人维护，可以采用较轻的季度节奏。

## 每季度至少完成

- 更新依赖；
- 运行 MSRV；
- 检查 Clippy；
- 检查 docs.rs；
- 检查 Snapshot 兼容；
- 检查所有 examples；
- 检查 README API；
- 复核开放 issue；
- 删除不再使用的实验代码；
- 更新 Roadmap。

## 每次发布前

```text
cargo fmt --check
cargo check
cargo check --features serde
cargo test
cargo test --features serde
cargo clippy --all-targets --features serde -- -D warnings
cargo doc --features serde --no-deps
cargo package
```

---

# 十九、功能进入核心库的最低门槛

任何算法或组件进入核心库前，至少需要：

- 一个明确实际用途；
- 一个可运行示例；
- 单元测试；
- 边界测试；
- 随机或参考对照测试；
- Serde 测试（若含状态）；
- reset 测试；
- 复杂度说明；
- 模型限制说明；
- CHANGELOG；
- 不破坏现有 API，或提供迁移说明。

不满足时，可以先放入：

```text
examples/
experiments/
benchmarks/
```

而不是直接公开稳定 API。

---

# 二十、最重要的后期改进顺序

如果资源有限，推荐严格按以下顺序：

```text
1. 数学正确性
2. Mira 实际接入
3. 基线比较
4. 冷启动和可信度
5. 模型诊断
6. Snapshot 稳定
7. Page-Hinkley
8. 稀疏特征
9. FTRL
10. ADWIN
11. Bandit
12. Python / WASM 生态
```

不要优先做：

```text
复杂树
GPU
神经网络
完整 River 兼容
```

---

# 二十一、阶段性成功定义

## v0.1 成功

```text
Mira 可以接入并稳定运行
```

## v0.2 成功

```text
Mira 可以知道模型何时可信、何时应回退
```

## v0.3 成功

```text
RillML 能服务设备数据之外的高维在线分类
```

## v0.4 成功

```text
数据规律变化后，模型能够检测并恢复
```

## v0.5 成功

```text
应用可以根据反馈自动选择策略
```

## v1.0 成功

```text
RillML 不再只是个人实验项目，而是可被外部 Rust 应用稳定依赖的基础库
```

---

# 二十二、最终原则

RillML 的长期发展应始终围绕：

```text
更正确
更可信
更容易接入
更容易诊断
更能适应变化
```

而不是：

```text
更多算法
更多模块
更复杂架构
```

最理想的长期形态是：

> 一个规模适中、边界清晰、每项能力都经过验证、可嵌入真实 Rust 应用的在线学习工具箱。
