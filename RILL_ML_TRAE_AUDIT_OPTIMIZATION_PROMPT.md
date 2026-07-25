# RillML 最新代码专项审计与确定性优化实施任务

你将对以下仓库进行一次基于最新代码的完整工程审计，并直接完成能够被代码、测试或基准证明有价值的优化：

- 仓库：https://github.com/hello-yunshu/rill-ml
- 默认分支：`main`
- 项目类型：Rust workspace
- 当前定位：轻量、可序列化、适用于 Rust 应用、边缘设备和持续数据流的在线机器学习库，同时包含签名模型包、独立 Runtime、稳定 IPC 协议和可插拔 WASM handler。

本任务不是要求增加更多算法，也不是为了套用通用“最佳实践”而重构。核心目标是：

> 找出并修复仍然存在的确定性正确性问题、状态安全问题、运行时信任边界问题、发布可靠性问题和可证明的性能浪费，同时保持 API 简洁、默认内存有界、兼容性清晰。

---

## 一、开始前必须完成的仓库确认

先执行：

```bash
git status
git branch --show-current
git log -10 --oneline
git remote -v
```

然后确认：

1. 当前工作目录确实是 `hello-yunshu/rill-ml`。
2. 当前分支和远端 `main` 的关系。
3. 工作区是否存在用户未提交改动。
4. 最新提交中是否已经修复本提示词列出的部分问题。

处理原则：

- 不覆盖、删除或重置用户现有改动。
- 工作区不干净时，不得擅自 `reset --hard`、`clean`、强制切换分支或覆盖文件。
- 可以执行 `git fetch origin`。
- 只有工作区干净且不会破坏用户工作时，才可以 fast-forward 到最新 `origin/main`。
- 所有结论必须以实际最新代码为准。
- 本提示词中的问题是审计线索，不是未经验证就必须照搬的修改清单。
- 已经被最新提交完整解决的问题，应记录为“已解决”，不要重复修改。

---

## 二、Agent 分工方式

根据 Trae 当前能力使用主 Agent 和子 Agent。

最多同时运行两个子 Agent。可以并行执行独立的只读审计，但涉及同一文件或相互依赖的修改必须由主 Agent统一协调。

建议分工：

### 子 Agent A：核心 ML 正确性

负责审计：

- `src/models/`
- `src/metrics/`
- `src/stats/`
- `src/optim/`
- `src/pipeline/`
- `src/preprocessing/`
- `src/persistence/`
- 稀疏特征、FTRL、R²、分类指标、序列化状态验证和失败原子性。

### 子 Agent B：Runtime、WASM 与发布链路

负责审计：

- `crates/rill-runtime/`
- `crates/rill-runtime-protocol/`
- `crates/rill-handler-api/`
- `handlers/`
- `.github/workflows/`
- `scripts/`
- 模型包、handler 包、IPC、WIT、Wasmtime 沙箱、签名、发布和版本同步。

### 主 Agent

负责：

- 仓库整体结构与公共 API；
- 合并两个子 Agent 的证据；
- 识别跨模块影响；
- 决定实施优先级；
- 完成或协调代码修改；
- 运行完整验证；
- 维护变更文档；
- 防止子 Agent 重复修改、相互覆盖或引入不一致实现。

子 Agent 输出必须包含：

1. 文件路径和具体代码位置；
2. 可复现的问题路径；
3. 当前测试是否覆盖；
4. 推荐的最小修复方案；
5. 是否涉及状态格式、公共 API 或兼容性；
6. 不应修改的部分。

不要让多个 Agent 同时直接修改同一文件。

---

## 三、审计与实施顺序

必须按照以下顺序推进：

```text
仓库状态确认
→ 最新代码只读审计
→ 为每个问题建立证据
→ 给问题分级
→ 先补失败测试
→ 实施最小修复
→ 单模块验证
→ workspace 全量验证
→ 必要的基准测试
→ 文档与发布一致性检查
→ 最终审计报告
```

不得先大范围重构，再尝试证明重构有价值。

---

# 四、第一优先级：核心正确性与状态安全

## 4.1 审计并修复 FTRL 数值污染问题

重点检查：

- `src/models/ftrl.rs`
- `FtrlParam`
- `FtrlRegressor::learn`
- `FtrlClassifier::learn`
- 权重计算、梯度平方、累加器、截距状态和 `samples_seen`

验证以下风险是否仍存在：

1. 输入值本身有限，但中间运算溢出为 `Infinity` 或 `NaN`；
2. `gradient * feature` 溢出；
3. `gradient * gradient` 溢出；
4. `n_old + gradient²` 溢出；
5. `sigma`、`z`、分母或权重变为非有限值；
6. 更新多个特征时，前几个特征已经修改，后续特征失败，导致部分提交；
7. `samples_seen` 溢出发生在模型状态已修改之后；
8. 恶意或损坏的 serde 状态绕过构造函数验证。

### 修复要求

FTRL 的单次 `learn` 必须满足失败原子性：

> 返回 `Err` 时，参数、截距、特征表和计数都不得发生任何改变。

优先使用“计算下一状态，再一次性提交”的方式。不要依赖发生错误后回滚已经修改的状态。

建议设计：

```rust
fn next_updated(
    &self,
    gradient: f64,
    weight: f64,
    config: &FtrlConfig,
) -> Result<Self, RillError>
```

也可以使用等价但更清晰的实现。

所有关键中间结果都必须进行有限性检查。不要只检查外部输入。

### 测试要求

至少加入：

- 有限输入导致乘法溢出的回归测试；
- 第 N 个特征失败时前 N-1 个特征不被提交；
- 截距更新失败不提交特征状态；
- `samples_seen` 溢出不改变任何状态；
- 分类和回归两个 FTRL 都覆盖；
- serde 恶意状态中的非有限值、负累加器或无效配置被拒绝；
- 正常收敛、稀疏性和动态特征行为不回退。

不要通过简单克隆整个无限增长模型来掩盖设计问题，除非基准证明成本可接受。优先构造本次受影响条目的下一状态。

---

## 4.2 为 FTRL 增加动态特征增长边界

检查当前 FTRL 是否对新 `FeatureId` 无限制地插入映射。

项目定位要求默认适合长期运行，动态特征不能在不可信数据流下无限增长。

设计一个简单、明确、可测试的边界策略。

建议方案：

```rust
pub struct FtrlConfig {
    pub alpha: f64,
    pub beta: f64,
    pub l1: f64,
    pub l2: f64,
    pub max_features: Option<usize>,
    pub new_feature_policy: NewFeaturePolicy,
}
```

首版策略保持简单：

```rust
pub enum NewFeaturePolicy {
    Reject,
    Ignore,
}
```

约束：

- 不要立即实现复杂 LRU 或自动淘汰，除非仓库已有明确需求。
- 已存在特征不受上限影响。
- 达到上限后，行为必须稳定、文档化、可序列化、可测试。
- 批量出现多个新特征时必须预先判断，不能插入一部分后再失败。
- 默认值需要平衡兼容性和长期安全性。
- 若保持 `None` 为兼容默认值，必须在文档中明确无限增长风险，并考虑 Runtime 或产品集成层使用有限值。
- 若改变默认值，必须评估是否属于破坏性行为。

测试：

- 恰好到达上限；
- 超出一个；
- 单次样本同时包含多个新特征；
- 已有特征仍可继续训练；
- `Reject` 时失败原子；
- `Ignore` 时只更新已有特征；
- serde 往返；
- 配置恢复校验。

---

## 4.3 修复分类指标的 `samples_seen()` 契约

检查：

- `Precision`
- `Recall`
- `F1Score`
- 其他实现 `Metric` 的类型

`Metric::samples_seen()` 应表示调用成功并纳入计算的总观测数，不能用 TP、FP、FN 的局部和代替。

为三个指标增加真实总计数，确保：

- 真负样本也计入总观测数；
- 每次成功更新恰好加一；
- 计数溢出时整个更新失败且状态不变；
- `reset()` 清空所有状态；
- serde 恢复保持正确语义。

必须评估状态格式影响。

如果当前项目仍处于 1.0 之前，可以进行合理的状态结构修正，但需要：

- 在 CHANGELOG 中说明；
- 为旧状态选择“明确拒绝”或“明确迁移”；
- 不允许静默用 `TP + FP` 猜测总样本数，因为无法恢复真负数量。

补充 trait 一致性测试：

```text
对所有 Metric：
成功调用 N 次 update 后，samples_seen() 必须等于 N
```

---

## 4.4 将 R² 改为稳定的在线算法

检查当前 R² 是否使用：

```text
sum(y²) - n × mean(y)²
```

若是，改为 Welford 风格的在线均值和 `M2`：

```rust
mean_truth: f64,
m2_truth: f64,
ss_res: f64,
count: u64,
```

要求：

- 大偏移、小方差数据仍能正确计算；
- 完美预测得到接近 `1.0`；
- 使用样本均值预测时得到接近 `0.0`；
- 常量真实值时返回 `None`；
- 少于两个样本时返回 `None`；
- 所有更新失败原子；
- 中间差值、乘法和累加均进行有限性检查；
- 避免将轻微负的浮点误差直接当作有效负方差。

加入以下类型测试：

```text
y = 1_000_000_000_001
y = 1_000_000_000_002
y = 1_000_000_000_003
```

并与离线稳定算法对照。

---

## 4.5 统一分类概率的公开契约

检查：

- `OnlineBinaryClassifier`
- `SparseClassifier`
- LogisticRegression
- FTRL classifier
- Naive Bayes
- LogLoss
- 所有公开文档与测试

确认概率约定到底是：

```text
[0, 1]
```

还是：

```text
(0, 1)
```

由于浮点 sigmoid 在极端 logit 下可以精确返回 `0.0` 或 `1.0`，优先考虑将公开 trait 文档统一为 `[0, 1]`，并由 LogLoss 内部裁剪。

不得只修改一个实现。所有分类器、指标、文档、测试必须一致。

---

# 五、第二优先级：Runtime、WASM 和 IPC 信任边界

## 5.1 禁止将 guest 任意错误字符串直接透传给 IPC 客户端

检查：

- WIT `handler-error`
- `WasmInvokeHandler`
- `InvokeHandler` trait
- `RuntimeEngine`
- `map_invoke_error`
- v1/v2 IPC Error response

当前目标应是：

```text
guest typed error
→ host typed error
→ 稳定 IPC code
→ 固定、无敏感信息的客户端 message
```

不要再依赖字符串前缀解析，例如：

```text
handlerExecutionFailed: ...
handlerTimeout: ...
```

建议新增宿主侧类型：

```rust
pub enum InvokeError {
    InvalidInput,
    UnsupportedCapability,
    ExecutionFailed,
    Timeout,
    Trap,
    OutputTooLarge,
    InvalidOutput,
    Internal,
}
```

可以携带仅供宿主日志使用的内部 detail，但：

- 不得直接发送给 IPC 客户端；
- 必须限制长度；
- 不得暴露 Wasmtime backtrace；
- 不得暴露模型原文、密钥、路径、环境变量或完整输入；
- 客户端 `message` 使用固定文案；
- `retryable` 由错误类型决定。

同步更新：

- WIT 说明；
- Rust trait；
- built-in handler；
- WASM handler；
- runtime engine；
- IPC tests；
- README/RUNTIME 文档。

确保 v1 和 v2 都保持稳定。

---

## 5.2 修复 `metadata()` 和 `configure()` 的真实墙钟超时

检查 Wasmtime epoch ticker 的启动时机。

必须确保以下调用都受到独立的 fuel 和墙钟限制：

1. 组件实例化；
2. `metadata()`；
3. `configure()`；
4. 每次 `invoke()`。

重点验证：

- epoch ticker 是否在 `metadata()` 和 `configure()` 调用前已经运行；
- `metadata()` 与 `configure()` 是否错误地共用一份 fuel；
- 初始化失败时是否遗留后台线程；
- `Drop` 是否最多等待一个 tick 才退出；
- 多个 handler 实例是否会各自产生长期线程；
- deadline 是否按照“当前 epoch + deadline”正确重置。

推荐使用 RAII 管理 ticker 生命周期，初始化任何阶段失败时都能停止线程。

补充恶意 handler fixtures：

- metadata 无限循环；
- configure 无限循环；
- invoke 无限循环；
- 迅速耗尽 fuel；
- 输出超过限制；
- metadata 消耗大量 fuel 后 configure；
- trap 携带内部 backtrace；
- 错误字符串超长。

测试必须验证：

- 返回稳定错误类型；
- 不泄露 guest detail；
- 进程不永久阻塞；
- 不遗留线程；
- 后续正常 handler 仍可使用。

---

## 5.3 检查 WASM 运行时资源模型

确认并验证：

- 无 WASI、文件系统、网络、环境变量、stdio 和进程能力；
- 最大内存；
- 最大 table；
- 最大输入输出；
- 组件编译和实例化成本；
- 并发调用行为；
- Mutex poison 后的处理；
- handler 超时后 Store 是否仍可安全复用；
- trap 后实例是否需要重建；
- 单个恶意请求是否会导致所有后续请求永久失败。

不要为了并发而贸然移除 Mutex。首版串行调用是可以接受的，但文档必须准确。

---

# 六、第三优先级：发布可靠性和供应链

## 6.1 修复已有版本标签的 SHA 一致性校验

检查 `.github/workflows/auto-release.yml`。

当版本标签已经存在时，必须确认：

```text
existing_tag_sha == successful_ci_head_sha
```

若不相同：

- 立即失败；
- 明确提示版本标签不可移动；
- 要求增加版本号；
- 不得把旧标签当成当前 main 的“安全重试”；
- 不得自动移动标签。

加入脚本测试或 workflow helper 测试，覆盖：

- 新标签；
- 已存在标签且 SHA 相同；
- 已存在标签且 SHA 不同；
- 已有成功 release；
- 已有进行中 release；
- 旧失败 release 的安全重试。

---

## 6.2 明确 PyPI 发布政策

检查正式 release workflow。

当前如果缺失 `MATURIN_PYPI_TOKEN` 而任务成功退出，需要决定：

### 方案 A：Python 包属于完整正式版本

则：

- token 缺失必须失败；
- PyPI 发布失败必须使 release workflow 失败；
- 发布完成后验证 PyPI 版本可见。

### 方案 B：Python 包是独立可选发布

则：

- 拆成独立 workflow；
- 主 release 不应声称 Python 已发布；
- Release summary 必须显示 Python 状态；
- README 和 CHANGELOG 中避免模糊表述。

根据仓库当前产品承诺选择更一致的方案，不得继续“静默跳过并显示绿色”。

---

## 6.3 固定发布工具版本

检查并处理：

- `wasm-pack`
- `wasm-tools`
- `maturin`
- `pytest`
- 关键 GitHub Actions
- Python 构建依赖

要求：

- 不使用未固定版本的 `curl | sh` 作为签名发布链路的一部分；
- 固定工具版本；
- 能验证下载哈希时应验证；
- Python 使用明确的 requirements 文件；
- GitHub Actions 至少固定 major，安全敏感 Action 优先固定 commit SHA；
- 保留 Dependabot 自动升级能力；
- 在文档中记录工具链版本来源。

不要随意升级所有依赖。只处理可复现构建所需的工具固定。

---

## 6.4 扩展 Security workflow 覆盖路径

确认安全工作流是否覆盖：

```text
src/**
crates/**
handlers/**
scripts/**
models/**
.github/workflows/**
Cargo.toml
Cargo.lock
```

特别是：

- `handlers/**` 包含实际执行的 WASM Rust 代码；
- `scripts/**` 包含签名索引和发布资产逻辑；
- `models/**` 影响正式签名包。

检查 CodeQL 是否应加入 Python。

权限按 job 最小化：

- 默认 `contents: read`；
- CodeQL job 单独 `security-events: write`；
- RustSec job 只保留实际需要的权限；
- 不给无关 job 全局写权限。

---

# 七、第四优先级：确定可测的性能优化

## 7.1 优化 StandardScaler 热路径

审计 `StandardScaler::transform()` 当前是否：

- 每次调用完整 `validate()`；
- 多次遍历数组；
- 生成 variance 临时 Vec；
- 再生成 scale 临时 Vec；
- 再生成 output Vec。

先使用现有 Criterion 基准或新增最小基准，测量：

- d = 8
- d = 32
- d = 128
- d = 1024

比较：

1. 当前 `transform()`；
2. 合并为单次循环的实现；
3. 可复用输出缓冲区的 `transform_into()`。

优化原则：

- 信任边界处做完整状态校验；
- 热路径不重复验证由私有字段和安全反序列化已经保证的不变量；
- 使用 `debug_assert!` 保留开发期检查；
- 不降低非有限值和维度验证；
- 不删除公开 `validate()`；
- 不牺牲代码可读性换取无法测量的微优化。

可增加：

```rust
fn transform_into(
    &self,
    features: &[f64],
    output: &mut Vec<f64>,
) -> Result<(), RillError>
```

但必须保持现有 `Transformer::transform()` API 兼容。

只有在基准证明有明确改善且没有行为回退时才合入。

---

## 7.2 审计其他热路径分配

检查但不要盲目修改：

- LinearRegression 每次 learn 的 `grad_weights Vec`；
- LogisticRegression 每次 learn 的临时 Vec；
- Pipeline transform 输出；
- rolling metrics；
- FTRL BTreeMap 查找；
- WASM JSON 序列化。

优先级规则：

```text
基准证明
> 明确的 O(d) 重复分配
> 纯主观推测
```

不要只为了速度把清晰实现改成 unsafe。

项目核心应继续保持 safe Rust。

---

# 八、文档和版本一致性

## 8.1 修复 README 当前版本漂移

从根 `Cargo.toml` 的 `[workspace.package].version` 获取唯一版本。

检查并修复：

- README 中文安装示例；
- README 英文安装示例；
- “当前版本”文字；
- 模块总览标题；
- 生态扩展标题；
- ROADMAP 当前状态；
- RUNTIME 文档；
- Python 文档；
- 示例 manifest；
- handler manifest；
- 发布输入示例；
- CHANGELOG 链接。

安装示例优先使用：

```bash
cargo add rill-ml
cargo add rill-ml --features serde
```

减少 README 硬编码版本。

扩展 `scripts/sync_version.py` 或等价脚本，使版本同步可重复执行、幂等，并增加测试。

发布验证应能检测：

- README 仍把旧版本称为当前；
- manifest 版本不一致；
- Python 版本不一致；
- handler guest metadata 版本不一致；
- changelog 缺少当前版本。

不要粗暴替换文档中作为历史示例或兼容性测试使用的旧版本号。

---

## 8.2 处理硬编码测试数量

检查 README 中精确测试数量是否仍准确。

更推荐：

- 自动生成；
- 或改成不易过期的覆盖说明；
- 或在 CI summary 中展示动态统计。

不要手动猜测测试数量。

---

# 九、低优先级安全加固

在主要问题完成后再处理。

## 9.1 ZIP 压缩率边界

检查整数除法是否导致压缩率判断截断。

优先采用避免浮点数、避免溢出的比较方式，例如通过 checked multiplication 或等价安全逻辑。

保留：

- 单文件解压大小；
- 总解压大小；
- 压缩总大小；
- 文件数量；
- 路径白名单；
- 重复文件拒绝；
- checksum 精确覆盖；
- 签名验证。

加入边界测试：

- 正好等于最大比率；
- 高一个单位；
- compressed size 为零；
- 极大尺寸避免乘法溢出。

---

## 9.2 Release URL 校验

检查签名 release index 中 URL 的验证强度。

至少考虑：

- 合法 URL；
- HTTPS；
- 禁止用户名密码；
- 禁止空 host；
- 禁止 `file:`、`data:`、`javascript:` 等 scheme；
- 是否允许 localhost 应由测试或开发模式明确决定。

不要在没有迁移策略的情况下突然拒绝已有合法 index。

---

# 十、禁止事项

不得执行以下行为：

- 不得为了“看起来更现代”重写整个架构；
- 不得增加 GPU、深度学习或分布式训练；
- 不得机械复制 River 的算法；
- 不得引入动态万能 Pipeline；
- 不得无证据替换所有 BTreeMap；
- 不得引入 unsafe 作为性能捷径；
- 不得放松签名、checksum、路径或包大小限制；
- 不得删除兼容 IPC v1 的支持，除非仓库已有明确版本决策；
- 不得把正常安全错误改成 panic；
- 不得掩盖测试失败；
- 不得通过删除失败测试让 CI 通过；
- 不得顺便格式化或重写大量无关文件；
- 不得修改用户未提交内容；
- 不得生成大量没有执行价值的规划文档而不处理代码。

---

# 十一、测试与验证要求

修改过程中先运行局部测试，再运行全量验证。

至少执行：

```bash
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features --exclude rill-ml-python -- -D warnings
cargo test --locked --workspace --all-targets --all-features --exclude rill-ml-python
cargo test --locked --workspace --doc --all-features --exclude rill-ml-python
cargo check --locked --workspace --all-targets --all-features
```

根据仓库现有 CI 继续执行：

- MSRV 1.94 检查；
- WASM build；
- wasm-pack tests；
- echo handler 构建；
- malicious handler tests；
- runtime process integration tests；
- Python maturin build；
- pytest；
- release helper Python tests；
- cargo package dry-run；
- 文档构建；
- release version validation；
- sync version 幂等性测试。

对性能修改运行 Criterion，并保存修改前后结果。

若本机缺少某工具或平台能力：

- 不伪造通过结果；
- 记录未运行的命令、原因和 CI 对应 job；
- 尽量运行其余可运行项目；
- 不因为某个外部工具缺失就停止整个审计。

---

# 十二、提交组织

优先拆成以下逻辑提交或 PR：

## PR 1：core-correctness

- FTRL 数值安全；
- FTRL 失败原子；
- 动态特征上限；
- Metric samples_seen；
- 稳定 R²；
- 概率契约。

## PR 2：runtime-boundary

- typed InvokeError；
- guest detail 脱敏；
- metadata/configure/invoke 超时；
- 恶意 handler 测试；
- Runtime 文档。

## PR 3：release-hardening

- 标签 SHA 一致性；
- PyPI 发布策略；
- 工具版本固定；
- security workflow；
- 发布脚本测试。

## PR 4：docs-performance

- README/ROADMAP 版本同步；
- 测试统计维护；
- StandardScaler 基准和优化；
- ZIP/URL 低优先级加固。

如果当前任务环境不适合创建多个分支，至少保持提交边界清晰。

每个提交必须：

- 只包含相关修改；
- 测试与实现放在一起；
- 提交信息说明原因，而不是只写“fix”；
- 不混入无关格式变化。

---

# 十三、最终交付文档

在仓库中新增或更新：

```text
docs/audits/RILL_ML_LATEST_AUDIT_AND_OPTIMIZATION.md
```

文档包含：

## 1. 审计基线

- 审计日期；
- 分支；
- HEAD SHA；
- 是否与远端 main 同步；
- 工作区初始状态。

## 2. 问题表

每项必须包括：

| ID | 级别 | 模块 | 问题 | 证据 | 状态 |
|---|---|---|---|---|---|

状态只能使用：

- Confirmed
- Fixed
- Already Fixed
- Not Reproducible
- Deferred
- Rejected

## 3. 每项修复说明

包含：

- 原问题；
- 复现方式；
- 根因；
- 修改方案；
- 为什么选择该方案；
- 兼容性影响；
- 测试覆盖；
- 剩余限制。

## 4. 验证结果

列出每条实际执行命令及结果。

## 5. 性能结果

仅写实际测得的基准数据，不写估计百分比。

## 6. 未完成项目

说明：

- 为什么未完成；
- 是否阻塞发布；
- 推荐后续动作。

---

# 十四、最终回复格式

任务结束时向用户输出：

1. 当前审计的 commit SHA；
2. 实际确认的问题数量；
3. 已修复、已存在修复、延期和否决的数量；
4. 最重要的修复摘要；
5. 修改的主要文件；
6. 实际运行的测试；
7. 未运行的测试及原因；
8. 是否建议发布新版本；
9. 若建议发布，说明属于 patch、minor 还是需要延后到 1.0；
10. 审计文档路径；
11. `git status --short`；
12. 最新提交列表。

不要只回复“已完成”。必须让用户可以根据最终回复独立判断修改是否可靠。

---

# 十五、完成标准

只有同时满足以下条件才算完成：

- 所有高优先级问题都被最新代码重新验证；
- 已存在修复没有被重复实现；
- 确认的问题有失败测试；
- 修复后测试通过；
- FTRL 失败时不污染状态；
- 分类指标 samples_seen 契约统一；
- R² 在大偏移、小方差数据上稳定；
- guest 错误细节不再直接暴露给 IPC；
- metadata/configure/invoke 都有真实资源和时间边界；
- 已存在 release 标签不会错误指向旧 SHA 重试；
- README 和实际版本一致；
- 没有破坏用户未提交内容；
- 没有无关大规模重构；
- 审计文档完整；
- 最终工作区状态清楚。

现在开始执行。先检查仓库状态并只读审计最新代码，不要立即修改。
