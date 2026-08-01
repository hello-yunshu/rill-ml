# RillML × Idleleo 深度推进执行报告

日期：2026-08-02

分支：`work/idleleo-online-decision-phase2`

基线：`origin/main` at `776b26d19a1aa53871a276db9a3eee32ed3797ad`

目标版本：Stable `1.1.0`；Preview `0.15.0`

## 结论先行

本分支完成了通用在线决策、延迟反馈、可解释 LinUCB、可移植漂移状态、
漂移共识、特征/模型身份、带权学习、IPC V3、Stateful Handler ABI v2、
决策回放、Preview `LinUcbFast` 与 Python 检查工具。实现通过本机全工作区
编译、测试、真实 Wasmtime 组件、真实 PyO3 wheel/import、1.0 semver、API
baseline、WIT v1、状态 fixture、MSRV host 编译、文档零警告和 RustSec 审计。

本次不是“全部项目完成”：第 9 项仅交付有明确 caller bounds 的
`ClippedMean`。在线 median/MAD/robust-z 原型在对抗异常值测试中不满足质量
要求，已删除而不是伪装完成。性能延迟有 Criterion 证据，但 allocation
profile 未执行。远端 PR CI、发布产物和实际 Idleleo 产品集成均未执行。

## 起点与远端证据

- 从精确 `origin/main` commit
  `776b26d19a1aa53871a276db9a3eee32ed3797ad` 建分支，没有在旧本地分支上叠加。
- 基线 CI：run `30523280402` 成功。
- 基线 Security：run `30523280363` 成功。
- 基线 Docs：run `30523280371` 成功。
- 基线 Auto Release：run `30523647673` 失败；原因是不可变 `v1.0.0` 已存在而
  workspace 版本仍为 `1.0.0`，发布逻辑安全拒绝重复版本。这不是当前代码测试
  通过的替代证据。
- 开始执行时存在三项用户工作树变化：删除两个旧 TRAE prompt，并新增本 prompt。
  本次没有恢复或覆盖它们；它们作为 prompt 交接的一部分纳入最终文档提交。

## 1—14 项执行状态

| 项 | 状态 | 实际交付 |
|---|---|---|
| 1. LinUCB score breakdown | 通过 | per-arm exploitation、exploration bonus、total、全臂分数和 condition diagnostics；原随机 tie 行为保留。 |
| 2. DecisionLedger | 通过 | caller-clocked、pending/completed 上限、ID 幂等、重复/冲突/过期/代际校验、显式清理、失败原子反馈。 |
| 3. Detector portable state | 通过 | Page-Hinkley、ADWIN、KSWIN 独立 V1 DTO、严格 config/state 校验、黄金 fixture；原 detector serde 仍为 Preview。 |
| 4. DriftConsensus | 通过 | 有界投票、warming、incomplete、连续确认、clear hysteresis、cooldown、event merge、generation 和解释结果。 |
| 5. Feature/Model identity | 通过 | 有序 FeatureSchema、SHA-256 canonical hash、Algorithm/ModelDescriptor、容量限制、schema mismatch 和 golden fixture。 |
| 6. LinUCB numerical stability | 通过 | Stable 路径改用 Cholesky solve；update 在 commit 前做 SPD 检查；冻结字段不变。 |
| 7. Deterministic tie break | 通过 | 新增 lowest-index deterministic 模式；现有 `select` 的 reservoir random tie contract 不变。 |
| 8. Online quantiles | 通过 | 常量内存 P² 单/多分位数、bootstrap、极值、离线随机对照、serde 连续性；文档明确无 worst-case rank 保证。 |
| 9. Robust statistics | 部分通过 | 仅保留精确定义的 `ClippedMean`。online median/MAD/robust-z 未交付，原因见风险部分。 |
| 10. Weighted API | 通过 | 独立 weighted traits；mean、population variance、EW mean、MAE/MSE、linear/logistic/pipeline adapters；零权重为已验证 no-op。 |
| 11. IPC V3 | 通过（Preview） | 独立 envelope 和 Handshake/Health/Observe/Decide/Feedback/Inspect/Snapshot/Reset；deadline、identity、capability、generation、schema 和 1 MiB 上限。 |
| 12. Handler ABI v2 | 通过（Preview） | `rill:handler@2.0.0` stateful world；Runtime-owned checksum/schema/generation；no-WASI Wasmtime、fuel/epoch、memory/table/I/O limits。 |
| 13. Replay harness | 通过（Preview state） | 延迟、乱序、重复、过期、缺失反馈，checkpoint/restore、reward/regret/baseline/latency、deterministic digest。 |
| 14. Fast LinUCB | 通过（Preview implementation） | `LinUcbFast` 使用 inverse cache + Sherman-Morrison；与 Stable 分数误差测试、转换、serde、失败原子更新和 Criterion benchmark。 |
| Python 对照/检查 | 通过（Preview） | 真 wheel/import；score/replay binding；JSON/JSONL/CSV、score series、drift timeline、latency、baseline 和 offline quantile helper。 |

## API 变化

所有变化都是 additive。四个 Stable crate 相对 1.0.0 的
`cargo-semver-checks` 均为 196 pass、57 skip、0 failure。

- `rill-ml::bandit`：`LinUcbArmScore`、`LinUcbConditionDiagnostics`、新的
  `LinUcb` scoring/deterministic 方法，以及 Preview `LinUcbFast`。
- `rill-ml::decision`：bounded delayed-feedback ledger、IDs、receipts、outcomes、
  LinUCB atomic outcome helper。
- `rill-ml::descriptor`：feature constraints/schema/hash 与 model/algorithm identity。
- `rill-ml::drift`：三种 Stable portable V1 DTO 和 Preview consensus。
- `rill-ml::stats`：P² quantiles 与 `ClippedMean`。
- `rill-ml::weighted`：weighted statistics/metrics traits 与实现；linear、logistic、
  regression/classification pipeline 增加 weighted adapters。
- `rill-ml::replay`：bounded deterministic replay harness。
- `rill-runtime-protocol::v3`：独立 Preview V3 types/errors/schema。
- `rill-handler-api::v2`：独立 Preview ABI v2 constants。
- `rill-runtime`：Preview stateful runtime engine、snapshot 和 Wasmtime v2 host。
- Python：`LinUcb` score/replay binding 与 dependency-free replay tools。

重新生成的 API baseline 是纯新增：

- `rill-ml`: `+613 / -0`
- `rill-runtime-protocol`: `+122 / -0`
- `rill-handler-api`: `+8 / -0`
- `rill-runtime`: `+72 / -0`

## 状态格式与稳定性分类

### Stable

- 既有 26 个 Stable state types 的字段和语义未修改，v0.13.0 与 v1 fixture
  继续全部加载。
- 冻结 `LinUcb` 仍只有 `arm_count`、`feature_count`、`alpha`、
  `a_matrices`、`b_vectors`、`samples_seen`；没有插入 inverse cache 字段。
- 新增 `PageHinkleyPortableStateV1`、`AdwinPortableStateV1`、
  `KswinPortableStateV1`。V1 字段、严格恢复规则和 portable-v1 fixtures 是
  新的稳定承诺；后续 breaking change 必须新增 V2 DTO。
- IPC v1/v2 fixture、top-level `RUNTIME_API_VERSION = 2`、WIT v1 文件和
  WIT v1 normalized SHA-256 均未改变。

### Preview state/format

- `DriftConsensus`, `P2Quantile`, `P2Quantiles`, `ClippedMean`, `LinUcbFast`
- `DecisionLedger`, `FeatureSchema`, `ModelDescriptor`
- `WeightedMean`, `WeightedVariance`, `WeightedExponentiallyWeightedMean`,
  `WeightedMae`, `WeightedMse`
- `DecisionReplayHarness`
- IPC V3、Stateful Handler ABI v2 及其 Runtime-owned snapshot

`state-schema-manifest.toml` 与 `STABILITY.md` 同步声明 26 Stable、26 Preview、
3 Stable portable DTO；coverage 脚本已校验无重复、无交叉、fixture 和文档一致。

## 真实测试结果

| 门禁/命令 | 结果 |
|---|---|
| `cargo fmt --all --check` + `git diff --check` | 通过 |
| `cargo clippy --locked --workspace --all-targets --all-features --exclude rill-ml-python -- -D warnings` | 通过，0 warning |
| `cargo test --locked --workspace --all-targets --all-features --exclude rill-ml-python` | 通过；core 751 tests，workspace integrations、examples、benches compile/run 全通过 |
| `cargo test --locked --workspace --no-default-features --exclude rill-ml-python` | 通过；含 core 751、workspace integrations 与 doctests |
| rill-ml feature powerset：none / serde / bandit / all | 四组 `cargo check` 全通过 |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --all-features --no-deps` | 通过，0 rustdoc warning |
| Rust 1.94 `cargo check --locked --workspace --all-targets --all-features` | 通过，35.00s |
| 四个 Stable crate `cargo semver-checks ... --baseline-version 1.0.0` | 每个 196 pass、57 skip、0 failure |
| `scripts/generate_api_baseline.py --verify` | 四 crate 通过；更新 diff 为纯新增 |
| `scripts/check_runtime_public_api.py` | normal public API PASS；internal ticker import 按预期拒绝 |
| `scripts/check_state_fixture_coverage.py` | 通过；26 Stable、26 Preview、3 portable Stable |
| `scripts/check_wit_abi.py` | 通过；v1 hash `108a68dfd6bcf86e3b63ad630508b2bbf407d00e8634067366e53dbc257cc90c` |
| `python3.12 -m unittest discover -s scripts/tests -v` | 80 passed |
| Maturin release wheel + installed import + pytest | wheel `rill_ml_python-0.15.0-cp39-...arm64.whl` built/installed；13 passed |
| Stable toolchain `wasm32-unknown-unknown` compile | `rill-ml-wasm` 通过 |
| 真实 Stateful Handler v2 component | 5 passed；normal/repeat、metadata mismatch、invalid/oversize、trap/timeout/output fail-closed |
| 真实 WIT v1 / malicious WASM runtime suite | 6 WIT v1 + 17 sandbox integration + 5 process tests 通过 |
| `cargo audit` with 1178 advisories / 409 dependencies | 最终 0 vulnerabilities |

### 执行中发现并修复的问题

1. no-default lib test 暴露测试对 optional `rand` 的隐式依赖；加入 dev-dependency。
2. descriptor golden hash 初值错误；用 canonical bytes 的真实 SHA-256 修正。
3. experimental online median/MAD 在对抗 outlier 流上污染不可接受；删除该实现，
   不以弱测试宣称 robust-z 完成。
4. external benchmark 无法构造 `#[non_exhaustive]` config literal；改用
   `Default` + field updates。
5. default-feature benchmark 暴露 replay 误依赖 serde-only trait；为 `LinUcb` 和
   ledger 提供 feature-independent inherent validation。
6. rustdoc `-D warnings` 抓到 V3 broken intra-doc link；修正并重跑通过。
7. RustSec 首轮发现 Wasmtime `46.0.1` 的 RUSTSEC-2026-0222 和
   RUSTSEC-2026-0223；锁文件升级到 `46.0.2` 后 audit 清零，并重跑 Runtime/WASM。
8. 升版后旧本地 echo/malicious component 自报 `1.0.0`，真实运行测试拒绝
   metadata mismatch；重建 1.1.0 components 后 17 项 sandbox suite 通过。

### 明确未通过或未验证

- 原始 `cargo test --workspace --no-default-features` 在 macOS 直接测试 PyO3
  cdylib 时因未通过 maturin 配置 Python C symbol linkage 而失败。核心 workspace
  在排除 Python crate 后通过；Python crate 已由真实 maturin wheel/install/import
  和 13 个 pytest 独立通过。不能把这两条入口混称为同一项通过。
- Rust 1.94 的 `wasm32-unknown-unknown` std target 起初未安装；补装下载长时间
  无进展后中止。因此 Rust 1.94 host MSRV 通过，WASM 在本机 stable Rust 1.96
  通过，但“Rust 1.94 + wasm32 target”组合未验证。
- allocation profiler 未执行；只记录了 Criterion 时间和按数据结构计算的
  f64 state payload。不能宣称零分配或给出分配次数。
- 当前开发分支尚未跑远端 PR 多平台 CI，也没有构建或验证任何发布资产。

## Benchmark 原始结果

命令：`cargo bench --bench linucb`

工具：Criterion 0.5，默认 100 samples，release profile，LTO，codegen-units=1

Rust：`rustc 1.96.0 (ac68faa20 2026-05-25)`

机器：Apple M5 / arm64，10 cores，32 GB，macOS 26.6

| 操作 | arms/features | Stable 95% estimate | Fast 95% estimate | 中位估算比 |
|---|---:|---:|---:|---:|
| select | 2 / 8 | 858.15–867.69 ns | 160.40–162.43 ns | 5.34× |
| select | 8 / 32 | 39.397–39.841 µs | 5.7126–5.7839 µs | 6.89× |
| select | 8 / 128 | 1.4960–1.5034 ms | 133.75–134.75 µs | 11.17× |
| update | 4 / 8 | 430.25–432.51 ns | 244.65–246.64 ns | 1.76× |
| update | 4 / 32 | 4.5239–4.5734 µs | 1.6398–1.6626 µs | 2.76× |
| update | 4 / 128 | 180.17–181.75 µs | 25.779–26.006 µs | 6.98× |

Criterion 报告存在若干 outliers，因此保留区间而不只报单点。Stable/Fast
随机训练后的 score breakdown 逐项误差测试阈值为 `1e-8`，从 Stable state
转换后 total score 阈值为 `1e-10`。

按显式 f64 payload 计算，Stable 和 Fast 当前都为每臂 `d² + d`：2 arms / 8
features 为 144 f64（1,152 bytes）；8 / 32 为 8,448 f64（67,584 bytes）；
8 / 128 为 132,096 f64（1,056,768 bytes）。这不含 Vec/allocator overhead，
也不是 allocation profile。

## 三轮自审

### 第一轮：稳定性

- `origin/main` 对比确认 WIT v1 两份 canonical copy、IPC v1/v2 fixtures 无 diff。
- protocol top-level version 仍为 2；V3 只通过新 module 暴露。
- LinUCB frozen serde field list 原样保留；fast inverse state 放入独立 Preview type。
- API baseline diff `+815 / -0`，四个 semver checks 通过。
- version sync 二次运行 0 updates；Stable 1.1.0 / Preview 0.15.0 一致。

结论：通过。

### 第二轮：可靠性

- 复核 finite/overflow/dimension/config/schema/generation/capacity 分支。
- 复核 ledger duplicate/conflict/expiry/out-of-order/idempotency、state corruption、
  replay continuity、atomic clone-apply-commit 和 Runtime fail-closed。
- 全量 property/offline/random/extreme-value/serialization/cross-version tests 通过。
- RustSec 问题已修复到 Wasmtime 46.0.2 并重跑真实组件。

结论：通过；online median/MAD 因未达质量线不在交付范围。

### 第三轮：产品边界

- 新核心模块没有 Xray/Sentinel 业务名、网络 client、process invocation 或系统权限。
- consensus 只产出信号，不执行动作；DecisionLedger 只记录 caller 提供事实。
- Stateful v2 linker 明确无 WASI 和 ambient authority。
- 所有动态集合都有显式条目/字节/维度上限；raw context 是否含隐私仍由调用方
  负责最小化，RillML 不上传或隐式持久化到外部系统。
- Preview/Stable 在源码、manifest 和文档中分开；静态、真实运行和发布证据未混称。

结论：通过。

## 修改文件清单（分组）

相对 `origin/main` 的最终统计：81 files changed，10,945 insertions，2,496
deletions。删除量主要来自两个已被当前 prompt 取代的旧 TRAE prompt。

- Core：`src/decision/`, `src/descriptor.rs`, `src/replay.rs`,
  `src/weighted.rs`, `src/bandit/linucb.rs`, drift/stats/model/pipeline modules。
- Core tests/bench/fixtures：`benches/linucb.rs`, portable drift integration，
  descriptor golden fixture，portable-v1 state fixtures。
- Protocol/Handler/Runtime：protocol V3 source/schema/fixtures，handler WIT v2，
  Runtime WIT v2/stateful host/tests，stateful v2 guest fixture，CI component job。
- Python：binding source、13 tests、`tools/rill_replay.py`。
- Compatibility/release：four API baselines、state schema manifest/coverage script、
  workspace/preview manifests、locks、release plan、example manifests。
- Docs/contracts：README 中英、CHANGELOG、STABILITY、RUNTIME、HANDLER-RFC、
  ROADMAP、本 prompt、本报告；删除两个被本 prompt 取代的旧 TRAE prompt。

## 风险与回滚

- `LinUcbFast` 是 Preview；出现数值或兼容问题时直接切回 Stable `LinUcb`，其
  状态没有被 Fast 格式替换。
- IPC V3 / Handler ABI v2 是 opt-in Preview；可以停止注册 V3/v2 host 并继续
  使用冻结 IPC v1/v2 与 WIT v1。
- Portable detector DTO V1 一旦合并即为稳定承诺，不能通过改字段回滚；必须新增
  V2。若实现有问题，可暂停售出 export/restore 使用而不破坏已有 V1 数据。
- Wasmtime 46.0.2 是安全修复，不建议回退到 46.0.1；若出现新回归，应升级到
  同一 46.x 的后续安全 patch 并重跑真实 component suite。
- 第 9 项不能扩展为 robust-z，除非新算法有对抗 contamination、offline reference、
  state continuity 和 bounded-memory 证据。
- 整个分支可按提交逆序回滚；未合并 main、未创建 tag、未发布 crates。

## 提交与交付

计划/实际提交主题按职责拆分：

1. `feat(core): add explainable bounded online decision primitives`
2. `feat(runtime): add preview protocol v3 and stateful handler v2`
3. `release: advance stable APIs to 1.1.0`
4. `docs: record Idleleo advancement evidence and boundaries`

最终 commit hash 以分支 `git log` 为准。源码 ZIP 从最终已提交 `HEAD` 使用
`git archive` 生成；SHA-256 写入相邻 `.sha256` 文件，并在交付消息中提供。

## 最终分级判定

| 等级 | 判定 | 说明 |
|---|---|---|
| A. 已设计 | 通过 | 版本化、bounded、failure-atomic、Stable/Preview 边界已写入代码与文档。 |
| B. 已编码 | 不通过（全 prompt） | 交付范围已编码；第 9 项 median/MAD/robust-z 明确未交付。 |
| C. 已静态检查 | 通过 | fmt、diff-check、clippy、rustdoc、API/state/WIT scripts。 |
| D. 已真实编译 | 通过 | workspace/all-targets/all-features、MSRV host、WASM、wheel。 |
| E. 已单元测试 | 通过 | Rust/Python 单元、property、offline、corruption tests。 |
| F. 已集成测试 | 通过 | real process、WIT v1、Wasmtime v1/v2 components、PyO3 import。 |
| G. 已兼容性测试 | 通过（本地） | semver、pure-add API baseline、state fixtures、IPC v1/v2、WIT v1。 |
| H. 已性能测试 | 不通过（完整要求） | Criterion 时间已测且 Fast 显著更快；allocation 未验证。 |
| I. 已发布验证 | 不通过 | 未发布、未建 tag、未验证远端 release assets。 |

### 是否推进

- 是否适合作为开发分支继续推进：**通过**。
- 是否适合提交 PR：**通过**；PR 应触发远端多平台 CI。
- 是否适合合并 main：**不通过**；当前分支尚无 PR CI/审阅证据，且 prompt 全项
  与完整 performance criteria 未全部通过。
- 是否适合发布 1.1.0：**不通过**；未合并、未跑 release workflow、未验证发布资产。
- 是否已经满足 Idleleo Route Observe：**不通过（端到端）**；RillML 侧通用
  observe/decide/feedback/replay/inspect 基元已通过本机验证，但没有 Idleleo 产品
  集成、真实流量、隐私策略和产品级验收。
- 是否已经满足 Idleleo Route Assist：**不通过**；本分支刻意不让模型执行动作，
  也没有产品侧授权、策略、回滚和安全验收。
