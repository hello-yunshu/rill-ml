# RillML × Idleleo Rill 深度推进提示词

你现在负责严谨推进 GitHub 仓库：

https://github.com/hello-yunshu/rill-ml

目标：基于当前 `main` 的真实最新代码，继续推进 RillML，使其更好地支撑 Idleleo Rill、Mira 以及其他长期运行的本地在线学习产品。尽量完成下面第 1—14 项；至少应完成第 1—10 项，第 11—14 项在不破坏稳定接口、测试和发布质量的前提下继续推进。

这不是为 Xray 单独定制算法库。所有新增能力必须保持通用，不得把 Xray、Nginx、Reality、SNI、Fail2ban、systemd、Route Assist、root helper、后端地址、配置文件生成等业务概念放进 `rill-ml` 核心。

---

# 一、总体工作方式

1. 从远端最新 `main` 开始，确认工作树干净、版本、标签、CI 状态和当前公开 API。
2. 创建独立开发分支，例如：

```text
work/idleleo-online-decision-phase2
```

3. 允许使用子 Agent，但最多两个并行：
   - Agent A：LinUCB、延迟反馈、漂移与统计；
   - Agent B：Protocol、Runtime、Handler ABI、Python 对照及测试；
   - 主 Agent 负责架构边界、交叉审查、合并、完整测试和最终结论。
4. 不要只根据 README 推断能力，必须检查真实源码、测试、Schema、fixture、API baseline、状态清单和 CI。
5. 每完成一个阶段立即测试。有失败就定位、修复、重新执行，不能把失败留到最后统一处理。
6. 不要因为任务很多而省略关键实现、测试或文档。可以减少重复拉取、重复构建和无意义扫描。
7. 不得虚构：
   - 编译通过；
   - 测试通过；
   - crates.io 发布；
   - GitHub Actions 全绿；
   - 性能提升；
   - 跨版本兼容；
   - 数值精度；
   - 安全验收。
8. 若环境缺少某项工具，先尝试使用现有 Rust、Docker、GitHub Actions 或本机可用环境补齐；确实不能执行时，必须在报告中明确标为“未验证”，不能用静态检查替代真实执行。
9. 不要等待人工逐步确认。除非遇到无法安全决定的破坏性版本策略，否则自行采用最保守、兼容的方案继续推进。
10. 所有改动完成后，形成清晰提交；不要把临时日志、构建缓存、私钥、测试生成目录或无关格式化改动提交进仓库。

---

# 二、不可破坏的稳定边界

必须先阅读并遵守：

```text
STABILITY.md
RUNTIME.md
RELIABILITY.md
ROADMAP.md
CHANGELOG.md
state-schema-manifest.toml
api-baseline/
crates/rill-runtime-protocol/
crates/rill-handler-api/
```

硬性要求：

1. 不破坏 `rill-ml` 1.x 已冻结的公开 Rust API。
2. 不修改 IPC v1/v2 的既有字段、枚举、语义和错误码。
3. 不修改 WIT ABI v1 的现有世界、类型和函数语义。
4. 不直接改变已经冻结的 LinUCB 持久化字段。
5. 不移除、改名或改变现有 Stable 状态类型的序列化含义。
6. 新能力优先采用：
   - additive inherent method；
   - 新 trait；
   - 新模块；
   - 新版本 DTO；
   - IPC V3 类型；
   - Handler ABI v2；
   - Preview 类型。
7. 如果确实需要新缓存字段：
   - 使用 `serde(skip)`；
   - 或建立独立 Preview 新类型；
   - 不得静默改变 Stable 状态格式。
8. 核心库保持同步、轻量、无隐式线程、无隐式网络、无业务系统权限。
9. 所有动态内存结构必须有明确容量上限。
10. 所有恢复自不可信状态的入口必须验证：
    - 版本；
    - 维度；
    - 数值有限性；
    - 数组长度；
    - 计数一致性；
    - 容量上限；
    - 语义不变量。

---

# 三、阶段 A：LinUCB 可解释决策

## 1. 增加 LinUCB score breakdown

当前 LinUCB 内部已经计算 exploitation、exploration bonus 和 total score，但公开接口只返回 arm。

新增兼容性 API，例如：

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct LinUcbArmScore {
    pub arm: usize,
    pub exploitation: f64,
    pub exploration_bonus: f64,
    pub total_score: f64,
}

impl LinUcb {
    pub fn score_arm(
        &self,
        arm: usize,
        context: &[f64],
    ) -> Result<LinUcbArmScore, RillError>;

    pub fn score_all(
        &self,
        context: &[f64],
    ) -> Result<Vec<LinUcbArmScore>, RillError>;

    pub fn select_with_scores(
        &self,
        context: &[f64],
        rng: &mut impl Rng,
    ) -> Result<(usize, Vec<LinUcbArmScore>), RillError>;
}
```

具体名称可根据现有 API 风格调整，但必须满足：

- 不修改现有 `ContextualBandit::select()`；
- 与现有选择结果完全一致；
- exploitation 与 exploration 分开；
- 明确 exploration bonus 是否已乘以 alpha；
- total score 与真实选臂逻辑一致；
- tie 时保持现有随机或确定性策略；
- 无重复矩阵求逆等明显浪费；
- 所有输出必须为有限值。

测试至少覆盖：

- 单 arm；
- 多 arm；
- 相同得分；
- 未训练状态；
- 训练后得分；
- 非有限 context；
- 错误维度；
- 无效 arm；
- score 与 select 的一致性；
- serde 恢复前后得分一致。

---

# 四、阶段 B：通用延迟反馈

## 2. 新增 DecisionLedger

实现通用、业务无关、可序列化的延迟反馈原语。建议位置：

```text
src/decision/
```

建议类型：

```rust
DecisionId
PendingDecision<C, A>
DecisionOutcome
DecisionLedger<C, A>
DecisionReceipt
FeedbackStatus
```

至少支持：

- 注册新决策；
- 决策 ID 唯一；
- 相同事实的幂等重放；
- 相同 ID、不同事实必须拒绝；
- 保存 action、context、created_at、expires_at、model_generation；
- 接收延迟 reward；
- 未知反馈拒绝；
- 重复反馈拒绝；
- 反馈早于决策拒绝；
- 过期反馈拒绝；
- generation 不匹配拒绝；
- action 不匹配拒绝；
- 已完成决策 tombstone；
- 有界 pending 容量；
- 有界 completed/tombstone 容量；
- 不得静默逐出尚未完成决策；
- 只有调用方明确执行清理时才允许清除过期项；
- 清理结果必须返回被清除数量或 ID；
- feedback 校验失败不能先删除 pending；
- 任何更新失败保持状态不变；
- serde；
- `ValidateState`；
- 状态恢复校验；
- 不依赖系统时钟，由调用者传入时间。

为 LinUCB 提供可选帮助层，但不要把账本强绑定到 LinUCB：

```rust
DelayedContextualDecision
apply_outcome(...)
```

算法库不定义 reward 的业务意义。

必须加入：

- 普通单元测试；
- property test；
- 损坏状态恢复测试；
- 极限容量测试；
- ID 冲突测试；
- feedback 重放测试；
- 过期边界测试；
- 故障原子性测试；
- 序列化往返测试。

---

# 五、阶段 C：漂移状态与共识

## 3. Page-Hinkley、ADWIN、KSWIN portable state

当前三类漂移检测器可以使用 serde，但其内部状态不属于稳定状态兼容承诺。

不要立即冻结所有内部字段。优先新增显式 portable state：

```rust
PageHinkleyPortableStateV1
AdwinPortableStateV1
KswinPortableStateV1
```

为每类检测器增加类似：

```rust
export_state_v1()
restore_state_v1(config, state)
```

要求：

- portable state 有明确版本；
- 字段是恢复算法连续性真正需要的最小集合；
- 不把可重建缓存写入稳定 DTO；
- 恢复时严格验证有限值、计数、窗口长度、配置一致性；
- 错误状态不能部分恢复；
- 导出再恢复后，后续输入产生完全一致结果；
- 增加跨版本黄金 fixture；
- 在 `state-schema-manifest.toml` 中明确它们的 Stable/Preview 定位；
- 更新 STABILITY 和 CHANGELOG；
- 不误称现有原始 serde 内部结构已经稳定。

## 4. 新增 DriftConsensus

实现通用漂移共识器，不加入 Sentinel 或 Xray 概念。

建议能力：

```rust
DriftVote
DriftConsensusConfig
DriftConsensus
DriftConsensusResult
DriftEventSummary
```

至少支持：

- 多检测器投票；
- minimum votes；
- warning 与 drift 分级；
- 连续窗口确认；
- clear windows；
- 滞回；
- cooldown；
- event merge horizon；
- warming；
- 重复事件合并；
- 配置 generation/change context；
- 检测器暂时缺失或数据不完整；
- 可解释输出：哪些检测器触发、连续次数、剩余冷却；
- 有界状态；
- serde；
- `ValidateState`；
- reset。

禁止：

- 自动重置业务模型；
- 自动执行动作；
- 引入网络或安全产品概念。

测试必须覆盖：

- 1/3、2/3、3/3 投票；
- warning 到 drift；
- drift 到 clear；
- 冷却期；
- 连续窗口中断；
- 配置变更上下文；
- 不完整数据；
- 重复事件；
- 恢复状态后连续性；
- 非法 persisted state。

---

# 六、阶段 D：特征与模型身份

## 5. FeatureSchema 与 ModelDescriptor

新增通用特征语义和模型身份封装：

```rust
FeatureDescriptor
FeatureSchema
FeatureSchemaHash
ModelDescriptor
AlgorithmDescriptor
```

至少记录：

- 特征名称；
- 顺序；
- 单位；
- 变换；
- 可选范围或约束；
- schema version；
- 确定性 schema hash；
- algorithm name；
- algorithm/state version；
- feature schema hash；
- 可选 configuration digest。

要求：

- hash 生成确定性；
- 字段顺序或特征顺序改变必须改变 hash；
- JSON map 顺序不得影响同一语义的 hash；
- 恢复模型状态时可显式验证 schema；
- 不自动推断业务含义；
- 不保存隐私数据；
- 长度和字符串大小有上限；
- serde；
- `ValidateState`；
- fixture 和 property test。

---

# 七、阶段 E：LinUCB 数值稳定性与性能

## 6. 数值稳定性强化

在不破坏 Stable 持久化字段的前提下推进：

- 用 Cholesky solve 替代显式完整矩阵逆，或提供兼容实现；
- 检查正定性；
- 对数值退化给出明确错误；
- 增加条件状态诊断；
- context 与 reward 有限性检查；
- 大数和极小数测试；
- 长期更新测试；
- 对照可靠的离线矩阵计算；
- 随机 property test；
- 保证 select 不修改学习状态；
- update 任意失败保持状态原子性；
- 缓存只能 `serde(skip)` 或放入新 Preview 类型。

## 7. 确定性 tie-break 模式

保留现有随机 tie-break 行为，新增可选确定性评分/选择接口，便于：

- 状态回放；
- 跨平台测试；
- 决策审计；
- 线上影子比较。

不要改变现有 `select()` 的既有语义。

---

# 八、阶段 F：在线稳健统计

## 8. 在线分位数

至少实现一种经过验证的有界在线分位数算法，优先：

- P² quantile；
- 或 Greenwald–Khanna。

要求：

- 明确算法和误差边界；
- 支持单个 quantile；
- 可选多个 quantile；
- 有界内存；
- reset；
- samples_seen；
- serde；
- `ValidateState`；
- 与离线排序结果对照；
- 单调流、常量流、随机流、极端值测试；
- 非有限输入拒绝；
- 文档明确不适用场景。

## 9. 稳健统计

在质量可保证时继续实现：

- median/MAD 的有界近似；
- Winsorized 或 clipped statistic；
- robust z-score；
- 可解释误差或限制。

不要为了数量实现未经验证的算法。无法充分验证时，宁可只完成分位数并在报告中说明其余未做。

---

# 九、阶段 G：带权学习

## 10. Weighted API

不要直接修改已冻结 Stable trait。新增兼容 trait 或 adapter：

```rust
WeightedStatistic
WeightedOnlineRegressor
WeightedOnlineBinaryClassifier
```

优先支持：

- Mean/Variance/EWMean；
- LinearRegression；
- LogisticRegression；
- MAE/MSE；
- 必要的 Pipeline adapter。

要求：

- weight 必须有限且非负；
- 明确 weight=0 语义；
- 状态更新失败原子；
- 与重复样本或离线加权计算对照；
- serde 状态恢复；
- 不破坏现有无权重接口。

---

# 十、阶段 H：Runtime Protocol V3

## 11. 新增独立 IPC V3

不能修改 v1/v2。新增明确版本化类型，例如：

```rust
RuntimeRequestV3
RuntimeResponseV3
RuntimeErrorV3
EnvelopeV3
```

支持通用方法：

```text
Handshake
Health
Observe
Decide
Feedback
Inspect
Snapshot
Reset
```

至少包括：

- request ID；
- capability；
- client/runtime identity；
- deadline；
- feature schema hash；
- model generation；
- state generation；
- payload size limit；
- 可重试与不可重试错误；
- invalid JSON；
- unsupported capability；
- state mismatch；
- expired request；
- incompatible generation；
- duplicate feedback；
- handler timeout；
- output too large。

要求：

- deny unknown fields；
- 明确最大字符串和集合长度；
- 1 MiB 或更严格消息上限；
- v1/v2 fixture 完全不变；
- V3 自己建立 JSON Schema；
- 正反 fixture；
- fuzz/非法输入测试；
- 跨版本兼容说明；
- v1/v2 仍通过原有所有测试。

不要加入 Xray 的动作类型。

---

# 十一、阶段 I：Stateful Handler ABI v2

## 12. 新增 Handler ABI v2 Preview

不得修改 WIT v1。新增：

```text
rill:handler@2.0.0
```

或新的 stateful world。

核心模型：

```text
handler(event, current_state)
→ output
→ next_state
```

设计要求：

- event、state、output、next_state 都有严格字节上限；
- Handler 无文件系统；
- 无网络；
- 无进程；
- 无 Shell；
- 无系统时间；
- 无环境变量；
- 无随机源，除非由宿主显式提供确定性 seed；
- Runtime 负责状态校验、checksum、版本和持久化；
- Handler 只能返回建议和下一状态；
- 宿主决定是否执行动作；
- 超时、trap、超大输出、非法状态全部 fail-closed；
- v1 Handler fixture 仍能被当前 Runtime 加载；
- v2 作为 Preview，不误称已冻结 Stable。

至少增加：

- 正常状态更新 Handler；
- 返回损坏状态 Handler；
- 超时 Handler；
- trap Handler；
- 超大状态 Handler；
- metadata/version 不匹配 Handler；
- 状态恢复与重复调用测试。

---

# 十二、阶段 J：延迟反馈回放体系

## 13. 建立 Decision Replay Harness

这是将 Route、推荐、调度等真实产品接入 RillML 的关键。

新增通用回放工具或测试模块，能够输入：

```text
timestamp
decision_id
context
selected_arm
outcome_time
reward
generation
```

验证：

- select 时模型不更新；
- outcome 到达后才 update；
- 乱序反馈；
- 延迟反馈；
- 重复反馈；
- 丢失反馈；
- 过期反馈；
- generation 变化；
- Runtime 重启；
- ledger 序列化恢复；
- 特征 schema 改变；
- 漂移发生；
- 基线模型比较。

输出至少包括：

- cumulative reward；
- regret 或可解释近似；
- per-arm pulls；
- pending/completed/expired 数量；
- feedback latency；
- baseline comparison；
- deterministic replay digest。

提供通用示例，不使用 Xray 名称。

---

# 十三、阶段 K：高性能 LinUCB

## 14. 尽量完成性能版 LinUCB

在已有稳定 LinUCB 不可破坏的情况下，选择下面一种安全路线：

### 路线 A：优化现有内部实现

前提：

- 不改变 stable serialized fields；
- 不改变公开结果语义；
- 缓存使用 `serde(skip)`；
- restore 后可安全重建缓存；
- API semver 检查通过。

### 路线 B：新增 Preview `LinUcbFast` 或 `LinUcbV2`

可采用：

- Sherman–Morrison；
- Cholesky factor update；
- 缓存 inverse/factor；
- batched score；
- allocation reuse。

要求：

- 与现有 LinUCB 在同一输入下做结果对照；
- 明确误差容忍；
- benchmark 小、中、大 feature 数；
- benchmark 多 arm；
- 长期更新数值稳定性；
- 状态大小；
- update/select 复杂度；
- 不只给出微基准数字，必须防止 benchmark 无意义优化；
- 性能没有真实提升时不得宣称提升。

---

# 十四、阶段 L：Python 对照与可视化

在前面阶段稳定后继续推进，不得反过来阻碍 Rust 核心。

利用现有 Preview Python crate 增加开发与验证工具：

1. Decision replay 数据读取；
2. LinUCB per-arm score 展示；
3. exploitation/exploration 曲线；
4. 漂移共识时间线；
5. delayed feedback latency；
6. baseline vs LinUCB 对照；
7. quantile/robust statistics 与 NumPy/Python 离线结果对照；
8. 导出 CSV/JSON，不建立强制 notebook 依赖。

Python 侧主要用于：

- 实验；
- 对照；
- 可视化；
- 调试。

不得把 Python 变成生产 Runtime 的依赖。

如环境无法构建 PyO3 wheel，可以完成 Rust 侧导出格式和 Python 源码检查，但必须明确 wheel 和实际 import 尚未验证。

---

# 十五、版本策略

不要一开始就批量修改版本号。

完成实现、测试、semver 检查后再决定：

- Stable crate 的 additive 新能力可进入 `1.1.0`；
- 发生变化的 Preview crate 可进入下一合理 `0.x`；
- IPC V3 为新增版本类型，不改变 IPC v1/v2；
- Handler ABI v2 先标记 Preview；
- Portable detector state 必须明确稳定性等级；
- 不发布到 crates.io，除非用户明确要求且所有发布门禁真实通过。

Runtime 或下游依赖仍应使用：

```toml
rill-ml = { version = "1", features = ["serde"] }
```

由发布时经过审查的 `Cargo.lock` 固定实际版本，不永久锁死 `=1.0.0`。

---

# 十六、完整测试门禁

必须尽量执行：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --workspace --no-default-features
cargo test -p rill-ml --features serde
cargo doc --workspace --all-features --no-deps
cargo semver-checks
cargo audit
```

并执行仓库现有：

- API baseline 检查；
- WIT ABI 检查；
- state fixture coverage；
- release pipeline 自检；
- feature powerset；
- WASM；
- Python；
- Runtime；
- Protocol；
- Handler；
- doctest；
- examples；
- benches 可编译检查。

若已有 `make`、`just`、脚本或 CI 统一入口，优先使用仓库原入口，避免绕开真实门禁。

还要增加：

- proptest；
- corrupted state；
- serialization round-trip；
- cross-version fixture；
- deterministic replay；
- random offline comparison；
- delayed and duplicate feedback；
- large dimensions；
- extreme numeric values；
- memory-bound assertions；
- malformed IPC；
- Handler timeout/trap/oversize；
- v1/v2/WIT v1 compatibility regression。

---

# 十七、性能测试要求

对新性能实现记录：

- Rust 版本；
- CPU/架构；
- build profile；
- feature 数；
- arm 数；
- 样本数；
- 每次 select/update 时间；
- 分配情况；
- 状态大小；
- 与当前 LinUCB 的相对结果；
- 误差。

不得只跑一次，不得把 debug build 与 release build 混比。

---

# 十八、最终自审

完成代码后至少进行三轮不同角度自审：

## 第一轮：稳定性

检查：

- 是否破坏 API；
- 是否改变冻结 serde 字段；
- 是否修改 IPC v1/v2；
- 是否修改 WIT v1；
- 是否漏掉 fixture；
- 是否版本标注错误。

## 第二轮：可靠性

检查：

- 非有限数；
- overflow；
- 维度错误；
- 状态损坏；
- 容量耗尽；
- 重复 ID；
- 延迟反馈；
- 乱序；
- 失败原子性；
- 恢复连续性；
- 幂等。

## 第三轮：产品边界

检查：

- 是否混入 Xray 业务；
- 是否让模型执行动作；
- 是否引入隐式网络或系统权限；
- 是否保存原始隐私数据；
- 是否把 Preview 误称 Stable；
- 是否把静态检查误称真实运行。

发现问题后直接修改并重新执行相关测试，不要只写在报告中。

---

# 十九、提交与交付

建议按阶段形成清晰提交，例如：

```text
feat(bandit): expose explainable LinUCB scores
feat(decision): add bounded delayed-feedback ledger
feat(drift): add portable detector state
feat(drift): add consensus and cooldown primitives
feat(schema): add feature and model descriptors
perf(bandit): add stable LinUCB scoring path
feat(stats): add bounded online quantiles
feat(weighted): add weighted online learning adapters
feat(protocol): add versioned runtime IPC v3
feat(handler): add preview stateful ABI v2
test(replay): add delayed-decision replay harness
perf(bandit): add preview high-performance LinUCB
feat(python): add replay inspection and comparison tools
docs: document Idleleo-compatible generic capabilities
```

最终交付：

1. 完整修改后的源码；
2. 清晰的 commit 列表；
3. 修改文件清单；
4. API 变化清单；
5. 状态格式变化清单；
6. Stable/Preview 分类；
7. 测试命令和真实结果；
8. 未执行测试及原因；
9. benchmark 原始结果；
10. 与 `main` 的 diff 统计；
11. 风险与回滚说明；
12. `RILL_ML_IDLELEO_ADVANCEMENT_REPORT.md`；
13. 如本地环境允许，生成完整源码 ZIP 和 SHA-256；
14. 推送到独立分支，但不要自动合并 `main`，不要自动发布 crates.io。

---

# 二十、最终判定格式

结论必须分级：

```text
A. 已设计
B. 已编码
C. 已静态检查
D. 已真实编译
E. 已单元测试
F. 已集成测试
G. 已兼容性测试
H. 已性能测试
I. 已发布验证
```

并分别判断：

```text
是否适合作为开发分支继续推进
是否适合提交 PR
是否适合合并 main
是否适合发布 1.1.0
是否已经满足 Idleleo Route Observe
是否已经满足 Idleleo Route Assist
```

没有真实证据时，必须明确写“不通过”或“未验证”，不得使用“基本完成”“理论可用”代替。

优先保证：

> 正确性、兼容性、状态可恢复性、结果可解释性和真实测试证据。

不要为了完成第 14 项而牺牲前 1—10 项的质量。若第 11—14 项无法达到同等质量，应交付前面已经完整通过门禁的内容，并准确记录剩余工作。
