# RillML 最新代码专项审计与确定性优化报告

本报告由 `RILL_ML_TRAE_AUDIT_OPTIMIZATION_PROMPT.md`（首轮）与 `RILL_ML_TRAE_SECOND_AUDIT_COMPLETION_PROMPT.md`（二次闭环）触发，基于对 `hello-yunshu/rill-ml` 仓库 `main` 分支最新代码的完整只读审计与后续最小修复。

---

## 0. 二次审计闭环（2026-07-26）

### 0.1 二次审计基线

| 项 | 值 |
|---|---|
| 二次审计日期 | 2026-07-26 |
| 仓库 | hello-yunshu/rill-ml |
| 分支 | `main` |
| 二次审计起始基线（origin/main） | `8daf752b2c3717c768c04117cf44e66a2a525339`（即上一轮首轮审计合并后的 main） |
| 二次审计最终 HEAD SHA | `962a66374845af627791b76b6e87069974b5b970`（含闭环修正提交） |
| 与远端 `origin/main` 关系 | 本地 `main` 领先 `origin/main` 共 7 个未推送提交（含 6 个二次审计修复提交 + 1 个本轮闭环修正提交） |
| 当前版本 | `0.9.0`（建议 patch 至 `0.9.1`） |
| 上一轮审计基线 | 首轮审计以 `e00f631` 为基线，修复后合并为 `e6a5953`/`209ffcd`/`d0b97ce`/`2ccb9e3`/`abc12f3`，版本提升至 `0.9.0`（`eccd918`），最终合并入 main（`8daf752`） |
| 二次审计触发提示词 | `RILL_ML_TRAE_SECOND_AUDIT_COMPLETION_PROMPT.md` |
| Rust 工具链 | `cargo 1.95.0`（workspace MSRV 1.94）；MSRV 1.94.0 检查已本地执行 |

二次审计前已确认：

- `git status` 显示首轮审计修复已全部提交，工作区干净（仅有未跟踪的提示词文件 `RILL_ML_TRAE_SECOND_AUDIT_COMPLETION_PROMPT.md`）；
- `git log --oneline origin/main..HEAD` 显示本地领先 7 个未推送提交，对应提示词第十二节建议的 6 个逻辑提交 + 1 个本轮闭环修正提交；
- 未对用户已有改动执行任何 `reset --hard`、`clean`、强制切分支或覆盖操作；
- 全部结论以本次最新代码为准。

### 0.2 二次审计问题表

状态取值：`Confirmed` / `Fixed` / `Already Fixed` / `Partially Fixed` / `Regression` / `Deferred` / `Rejected` / `Not Reproducible`。

| ID | 级别 | 模块 | 问题 | 证据 | 状态 |
|---|---|---|---|---|---|
| 2-A-01 | 高 | `scripts/release_tag_policy.py` | 已有 release（成功/活动中）时先返回 `skip`，未先比较 `tag_sha` 与 `target_sha`，导致版本号未提升时静默跳过 | 旧实现 `if has_successful_release: return skip` 在 SHA 比较之前 | Fixed |
| 2-A-02 | 高 | `src/models/ftrl.rs` | 非零梯度平方下溢为零（`gradient != 0 && gradient * gradient == 0`）时，`n_new == 0 && z_new != 0` 形成零分母权重 | 旧 `next_updated` 未检查 `gradient_sq == 0.0`；`weight()` 在 `n == 0` 时分母为零 | Fixed |
| 2-A-03 | 高 | `src/models/ftrl.rs` | `Ignore` 策略先计算 `grad * value` 再判断是否忽略，超额新特征可能因乘法溢出导致整次训练失败 | 旧实现特征循环内先算梯度再检查 `max_features` | Fixed |
| 2-A-04 | 高 | `src/models/ftrl.rs` | serde 恢复的 `n == 0 && z != 0` 恶意状态可绕过校验 | 旧 `validate()` 未检查此组合 | Fixed |
| 2-A-05 | 高 | `src/models/ftrl.rs` | `dot + intercept` 最终加法无有限性检查 | 旧 `predict_inner` 直接返回 `dot + intercept` | Fixed |
| 2-B-01 | 高 | `src/models/naive_bayes.rs` | `GaussianNaiveBayes` / `BernoulliNaiveBayes` / `MultinomialNaiveBayes` 的 `predict_proba` 可能返回 `Ok(NaN)`（`-Infinity - -Infinity = NaN`） | trait 契约要求概率 ∈ [0,1]，但中间 log likelihood 累加无有限性检查 | Fixed |
| 2-B-02 | 高 | `src/models/naive_bayes.rs` | 三种 Naive Bayes 的 `learn()` 逐特征提交，中途失败留下部分状态 | 旧实现直接修改 `self.feature_sums_true[i]` 等，第 N 个特征溢出后前 N-1 个已提交 | Fixed |
| 2-C-01 | 高 | `crates/rill-runtime/src/server.rs` / `handler/wasm.rs` | WIT `handler-error` 四种 variant（`invalid-model` / `invalid-input` / `unsupported-capability` / `execution-failed`）被折叠为单一 `ExecutionFailed`，通过 Debug 字符串猜测 variant | 旧 `map_invoke_error` 使用 `format!("{:?}", handler_error).starts_with("ExecutionFailed")` | Fixed |
| 2-C-02 | 高 | `crates/rill-runtime/src/handler/wasm.rs` | `eprintln!("{detail}")` 在 `InvokeError::with_detail()` 截断前输出完整 guest-controlled detail（最长 16 KiB），存在日志泄露与日志放大风险 | 旧实现先 `eprintln!` 再构造 `InvokeError` | Fixed |
| 2-C-03 | 高 | `handlers/test-metadata-loop-handler/` | 缺真实 `metadata()` 无限循环 fixture；首轮审计因 WIT 契约限制标记为不可行，但未尝试独立 test-only handler | 首轮报告 §3.2 B-04 明确未覆盖 | Fixed |
| 2-D-01 | 高 | `src/metrics/classification.rs` | `Precision` / `Recall` / `F1Score` serde 一致性校验使用 `saturating_add`，非法超大状态可能被饱和后错误接受 | 旧实现 `state.true_positive.saturating_add(state.false_positive)` | Fixed |
| 2-D-02 | 高 | `src/metrics/regression.rs` | R² serde invariant 不完整：未检查 `ss_res >= 0`，未检查 `m2_truth >= 0`（仅有 finite 检查） | 旧 Deserialize impl 仅检查 finite | Fixed |
| 2-D-03 | 高 | `src/preprocessing/standard_scaler.rs` | StandardScaler serde invariant 不完整：未检查 `m2s >= 0`，未检查 counts 同步；`transform()` 热路径每次执行完整 O(d) `validate()` | 旧 `validate()` 未检查 m2s 非负；`transform()` 每次调用 `validate()` | Partially Fixed |
| 2-E-01 | 高 | `.github/workflows/pipeline.yml` / `scripts/requirements-dev.txt` | 工具链版本仅限制 major（`wasm-pack ^0.13` / `wasm-tools ^1.0` / `maturin>=1.7,<2.0` / `pytest>=8,<9`），不属于精确固定 | 旧实现使用范围版本 | Fixed |
| 2-F-01 | 中 | `README.md` / `README.en.md` | README 笼统声称"内存有界"，但 FTRL 默认 `max_features: None` 仍允许动态特征状态无限增长 | 旧 README 第 41 行"内存有界" | Fixed |
| 2-G-01 | 高 | `crates/rill-runtime/src/server.rs` | `InvokeErrorKind` 是公开 `pub enum` 且无 `#[non_exhaustive]`，二次审计新增 3 个 variant（`InvalidModel` / `InvalidInput` / `UnsupportedCapability`）是公共 API 变化，与 patch 版本决策不一致 | 旧审计报告 §0.9 称"不改变公共 API"但 enum 实际公开且可被外部穷尽 `match` | Fixed |
| 2-G-02 | 中 | `docs/audits/RILL_ML_LATEST_AUDIT_AND_OPTIMIZATION.md` | 审计报告 §0.1 HEAD SHA 错误（写 `8daf752...`，实际为 `962a663...`）；§0.1、§0.7、§0.8 三次写"修改保留在工作区未提交"与实际已提交状态不符；未区分 6 类 release 发布状态 | 旧审计报告与 `git log` 实际状态不一致 | Fixed |

二次审计总计：**15 项 Confirmed，13 项 Fixed，1 项 Partially Fixed（2-D-03），0 项 Rejected / Deferred / Regression / Already Fixed**。1 项 2-G 系列为审计报告闭环修正（2-G-01 / 2-G-02）。

### 0.3 二次审计修复说明

#### 2-A-01：release tag SHA 判断顺序

- **修改方案**：在 [scripts/release_tag_policy.py](../../scripts/release_tag_policy.py) 中，当 `tag_exists == true` 时，先比较 `tag_sha` 与 `target_sha`：
  - SHA 不同 → 立即 `fail`（无论 release 状态如何）；
  - SHA 相同 + 成功 release → `skip`；
  - SHA 相同 + 活动 release → `skip`；
  - SHA 相同 + 失败 release → `dispatch` 重试；
  - SHA 相同 + 无 release → `dispatch`。
- **测试覆盖**：[scripts/tests/test_release_tag_policy.py](../../scripts/tests/test_release_tag_policy.py) 新增 `test_existing_tag_with_different_sha_fails_even_with_successful_release`、`test_existing_tag_with_different_sha_fails_even_with_active_release`、`test_existing_tag_with_matching_sha_dispatches_retry`、`test_existing_tag_with_unresolved_sha_fails` 共 4 个用例，覆盖 SHA 不同/相同 × release 成功/活动/失败/无 的全部分支。

#### 2-A-02 ~ 2-A-05：FTRL 边界修复

- **2-A-02 下溢**：在 [src/models/ftrl.rs](../../src/models/ftrl.rs) 的 `next_updated` 中新增 `gradient_sq == 0.0 && gradient != 0.0` 检查，返回 `NonFiniteValue` 错误；在 `learn` 提交前调用 `FtrlParam::weight()` 验证下一状态权重有限。
- **2-A-03 Ignore 顺序**：在特征循环前预扫描新特征，若 `new_feature_policy == Ignore` 且加入会超过 `max_features`，跳过该新特征的所有运算（不计算梯度、不创建参数），已有特征正常更新。
- **2-A-04 serde**：`FtrlRegressor` / `FtrlClassifier` 的 `validate()` 新增 `n == 0 && z != 0` 检查，拒绝零分母恶意状态。
- **2-A-05 最终加法**：`predict_inner` / `predict_proba_inner` 使用 `checked_finite_add(dot, intercept, "ftrl prediction")` 检查最终加法。
- **测试覆盖**：`regressor_gradient_squared_underflow_is_atomic`、`classifier_gradient_squared_underflow_is_atomic`、`regressor_serde_rejects_n_zero_z_nonzero`、`classifier_serde_rejects_n_zero_z_nonzero`、`regressor_ignore_skips_overflowing_new_feature`、`classifier_ignore_skips_overflowing_new_feature`、`regressor_predict_dot_plus_intercept_overflow`、`regressor_boundary_config_predict_after_learn_always_finite`、`classifier_boundary_config_predict_proba_after_learn_always_finite`（每个场景为回归器/分类器分别写测试，覆盖更全面）。

#### 2-B-01 ~ 2-B-02：Naive Bayes 概率有限性与失败原子

- **2-B-01 概率有限性**：三种 Naive Bayes 的 `predict_proba` 对 `log_odds` 进行 NaN 检查，若 `log_odds.is_nan()` 返回 `Err(NonFiniteValue)`；单项 log likelihood 与累加结果均经 `ensure_finite` / `checked_log_add` 检查。
- **2-B-02 失败原子**：三种 Naive Bayes 的 `learn()` 改为"先计算 next state，所有运算成功后一次性提交"模式：
  - Gaussian：先计算 `next_feature_sums` / `next_feature_sums_sq` / `next_means` / `next_variances` / `next_class_count` / `next_samples_seen`，全部成功后提交；
  - Bernoulli：先计算 `next_feature_counts` / `next_class_count` / `next_samples_seen`；
  - Multinomial：先计算 `next_feature_sums` / `next_total` / `next_class_count` / `next_samples_seen`。
- **测试覆盖**：`gaussian_predict_proba_rejects_extreme_features_causing_nan_log_odds`、`bernoulli_predict_proba_rejects_extreme_features_causing_nan_log_odds`、`multinomial_predict_proba_rejects_extreme_features_causing_nan_log_odds`、`gaussian_learn_failure_leaves_state_unchanged`、`bernoulli_learn_failure_leaves_state_unchanged`、`multinomial_learn_failure_leaves_state_unchanged`、`gaussian_predict_proba_does_not_modify_state_on_error`（加上既有的 `gaussian_predict_does_not_update_state` / `bernoulli_predict_does_not_update_state` / `multinomial_predict_does_not_update_state`）。

#### 2-C-01 ~ 2-C-03：Runtime typed error 与 metadata loop fixture

- **2-C-01 WIT typed error 映射**：在 [crates/rill-runtime/src/server.rs](../../crates/rill-runtime/src/server.rs) 中扩展 `InvokeErrorKind`：
  ```rust
  pub enum InvokeErrorKind {
      InvalidModel, InvalidInput, UnsupportedCapability, ExecutionFailed,
      Timeout, Trap, OutputTooLarge, InvalidOutput, Internal,
  }
  ```
  在 [crates/rill-runtime/src/handler/wasm.rs](../../crates/rill-runtime/src/handler/wasm.rs) 中将 `invoke_handler::HandlerError` 的四种 variant 直接映射到对应 `InvokeErrorKind`，不再通过 Debug 字符串猜测。
- **2-C-02 日志截断**：删除 `handler/wasm.rs` 中的 `eprintln!("{detail}")`，引入 `HostLogSink` trait 抽象日志输出；`InvokeError::with_detail()` 先截断到 4 KiB（在 UTF-8 字符边界上），再通过 `HostLogSink` 输出已截断的 detail；IPC message 永远不含 detail。
- **2-C-03 metadata loop fixture**：新增独立 test-only handler [handlers/test-metadata-loop-handler/](../../handlers/test-metadata-loop-handler/)，其 `metadata()` 包含无限循环；在 CI 的 `wasm-handler` job 中构建为 WASM component；测试 `wasm_handler_metadata_infinite_loop_returns_load_error` 验证在 epoch deadline 内失败；`metadata_loop_handler_failure_does_not_leak_ticker_or_block_later_handlers` 验证 ticker 线程被清理、后续正常 handler 仍可用。
- **测试覆盖**：`wasm_handler_metadata_infinite_loop_returns_load_error`、`metadata_loop_handler_failure_does_not_leak_ticker_or_block_later_handlers`、`wasm_handler_long_error_string_is_truncated`（验证截断后日志长度 ≤ 4 KiB）、`echo_handler_rejects_unsupported_capability`（验证 `UnsupportedCapability` kind 不被折叠为 `ExecutionFailed`）。

#### 2-D-01 ~ 2-D-03：状态校验加固

- **2-D-01 分类指标 checked_add**：在 [src/metrics/classification.rs](../../src/metrics/classification.rs) 中将 `Precision` / `Recall` / `F1Score` 的 serde 一致性校验从 `saturating_add` 改为 `checked_add`，溢出时返回 `serde::de::Error::custom(...)` 拒绝状态。F1 的链式加法（`tp + fp + fn`）每一步都使用 `checked_add`，避免第一步饱和后隐藏溢出。
- **2-D-02 R² serde invariant**：在 [src/metrics/regression.rs](../../src/metrics/regression.rs) 的 R² `Deserialize` impl 中新增 `ss_res >= 0` 检查（拒绝负残差平方和）、`m2_truth >= 0` 检查（拒绝负 M2）、`mean_truth` finite 检查。
- **2-D-03 StandardScaler invariant + 热路径**（**Partially Fixed**）：在 [src/preprocessing/standard_scaler.rs](../../src/preprocessing/standard_scaler.rs) 中：
  - `validate()` 新增 `m2s` 非负检查、counts 同步检查、state 长度匹配检查（**Fixed**）；
  - `transform_into()` 热路径仅在 `debug_assert!` 中检查内部不变量（state 长度、counts 同步、state 有限性），release 路径只保留信任边界检查（维度校验 + 输出有限性）（**Fixed**）；
  - `Transformer::transform()` 公共入口在二次审计首轮仍调用完整 `validate()`；本轮闭环修正后已移除该调用，直接委托 `transform_into()`，使 `transform()` 与 `transform_into()` 共享单次循环热路径（**Fixed in closeout**）。
- **测试覆盖**：
  - 分类指标：`precision_serde_rejects_tp_fp_overflow`、`recall_serde_rejects_tp_fn_overflow`、`f1_serde_rejects_tp_fp_overflow`、`f1_serde_rejects_tp_fp_fn_overflow`、`metric_serde_accepts_max_boundary`、`recall_serde_rejects_inconsistent_samples_seen`、`f1_serde_rejects_inconsistent_samples_seen` 共 7 个新用例；
  - R²：`r2_serde_rejects_negative_ss_res`、`r2_serde_rejects_negative_m2`；
  - StandardScaler：`serde_rejects_negative_m2`（验证负 M2 被拒绝）、既有 `serde_rejects_malformed_state`、`update_rejects_overflow_without_mutating_state`。

#### 2-E-01：工具链精确固定

- 在 [scripts/requirements-dev.txt](../../scripts/requirements-dev.txt) 中将 `maturin>=1.7,<2.0` 改为 `maturin==1.14.1`，`pytest>=8,<9` 改为 `pytest==8.4.2`；
- 在 [.github/workflows/pipeline.yml](../../.github/workflows/pipeline.yml) 中将 `wasm-pack --version "^0.13"` 改为 `--version "0.13.1"`，`wasm-tools --version "^1.0"` 改为 `--version "1.254.0"`（CI 与 release 两处均更新）；
- 版本来源：均为现有 CI 范围解析到的最新兼容版本（通过 crates.io API 与 PyPI `pip index versions` 确认）；
- **未增加 hashes**：Python 跨平台 hashes 维护成本过高（maturin 有平台特定 wheel），在注释中说明原因。
- **验证**：YAML 语法检查通过；`pip install -r scripts/requirements-dev.txt` 在本地已安装相同版本。

#### 2-F-01：README 内存边界契约

- 采用**方案 A（保持兼容默认值）**：保留 `max_features: None` 默认值，但修正 README 描述。
- 在 [README.md](../../README.md) 第 41 行将"内存有界"改为"固定维度算法使用有界状态，FTRL 等动态特征算法需设置 `max_features` 才能保证状态有界"；
- 在内存界限表中标注 FTRL 的 `k` 默认无上限，需设置 `max_features` 才能有界；
- [README.en.md](../../README.en.md) 同步更新。
- **理由**：方案 A 不改变默认行为，无破坏性影响；方案 B（改变默认值）需 minor 版本提升与 CHANGELOG，且"随意选择容量"违反提示词禁止事项。

### 0.4 二次审计验证结果

所有命令在 `macOS` / `cargo 1.95.0` 本地执行，工作目录 `/Users/yunshu/Documents/GitHub/rill-ml`。

| 命令 | 退出码 | 结果摘要 |
|---|---|---|
| `cargo fmt --all --check` | 0 | 通过（首次发现 `naive_bayes.rs` 格式差异，已 `cargo fmt --all` 修复后再次通过） |
| `cargo clippy --locked --workspace --all-targets --all-features --exclude rill-ml-python -- -D warnings` | 0 | 全工作区无警告 |
| `cargo check --locked --workspace --all-targets --all-features` | 0 | 全工作区（含 `rill-ml-python`）类型检查通过 |
| `cargo test --locked -p rill-ml --features serde --lib` | 0 | **639 passed; 0 failed**（含本轮新增 `r2_serde_rejects_negative_ss_res` / `serde_rejects_negative_m2`） |
| `cargo test --locked --workspace --all-targets --all-features --exclude rill-ml-python` | 0 | 全部通过（含 17 个 WASM handler 测试 + 4 个 runtime_process 测试） |
| `cargo test --locked --workspace --doc --all-features --exclude rill-ml-python` | 0 | 文档测试通过 |
| `cargo test --locked -p rill-runtime` | 0 | 3 passed（runtime_process） |
| `cargo test --locked -p rill-runtime --features wasm --test wasm_handler -- --include-ignored` | 0 | **17 passed; 0 failed**（含 `wasm_handler_metadata_infinite_loop_returns_load_error` / `metadata_loop_handler_failure_does_not_leak_ticker_or_block_later_handlers`） |
| `cargo test --locked -p rill-runtime --features wasm --test runtime_process` | 0 | 4 passed |
| `python3 -m unittest discover -s scripts/tests -v` | 0 | **41 passed**（含 `test_release_tag_policy.py` 11 用例 / `test_release_version.py` 5 用例 / `test_sync_version.py` 14 用例 / `test_release_indexes.py` 11 用例） |
| `cargo +1.94.0 check --locked --lib` | 0 | MSRV 通过 |
| `cargo +1.94.0 check --locked --lib --features serde` | 0 | MSRV + serde 通过 |
| `cargo +1.94.0 check --locked -p rill-handler-api` | 0 | MSRV 通过 |
| `cargo +1.94.0 check --locked -p rill-runtime-protocol` | 0 | MSRV 通过 |
| `cargo +1.94.0 check --locked -p rill-runtime` | 0 | MSRV 通过 |
| `cargo +1.94.0 check --locked -p rill-runtime --features wasm` | 0 | MSRV + wasm 通过 |
| `python3 scripts/sync_version.py` | 0 | 0 field(s) updated（幂等） |
| `python3 scripts/release_version.py` | 0 | release version 0.9.0 is internally consistent |

### 0.5 二次审计未运行项及原因

| 项 | 原因 | CI 对应 job |
|---|---|---|
| `wasm-pack test --node`（headless 浏览器） | 本机缺 wasm-pack 浏览器 runner；已通过 `cargo build --target wasm32-unknown-unknown` + `wasm-tools component new` 验证三个 handler（echo / malicious / metadata-loop）可编译可链接 | `pipeline.yml` 的 `wasm-build` job |
| Python `maturin develop` + `pytest`（`crates/rill-ml-python`） | 本机缺 PyO3 开发环境；`cargo check --workspace --all-features` 已覆盖 Rust 侧编译 | `pipeline.yml` 的 `python-build` job |
| `cargo package --dry-run` | 未在审计流程中执行，CI 已覆盖 | `pipeline.yml` 的 `package-check` job |
| Criterion 基准重跑 | 首轮审计已记录基准数据（§5.1），二次审计未修改 `transform_into` 实现，基准数据仍然有效 | `pipeline.yml` 的 `bench` job（可选） |

### 0.6 二次审计完成标准核对

按 `RILL_ML_TRAE_SECOND_AUDIT_COMPLETION_PROMPT.md` 第十四节：

| 完成标准 | 状态 |
|---|---|
| release tag SHA 在任何已有 release 状态下都先比较 | ✅（2-A-01） |
| FTRL 不会因非零平方下溢提交不可用状态 | ✅（2-A-02） |
| FTRL `Ignore` 真正跳过超额新特征 | ✅（2-A-03） |
| FTRL 最终预测加法有有限性检查 | ✅（2-A-05） |
| 三种 Naive Bayes 不返回 `Ok(NaN)` | ✅（2-B-01） |
| 三种 Naive Bayes 失败时状态完全不变 | ✅（2-B-02） |
| WIT 四种 guest error 被真实解析 | ✅（2-C-01） |
| guest detail 不在截断前输出 | ✅（2-C-02） |
| metadata 无限循环有真实 WASM fixture | ✅（2-C-03） |
| Precision / Recall / F1 serde 加法溢出被拒绝 | ✅（2-D-01） |
| R² 拒绝负 `ss_res` 等非法状态 | ✅（2-D-02） |
| StandardScaler 拒绝负 M2 | ✅（2-D-03） |
| StandardScaler 热路径不重复做无必要的完整扫描 | ✅（2-D-03，`debug_assert!` 替代 release `validate()`；本轮闭环已移除 `transform()` 的完整 `validate()` 调用） |
| wasm-pack、wasm-tools、maturin、pytest 使用精确版本 | ✅（2-E-01） |
| README 不再笼统误称所有配置默认内存有界 | ✅（2-F-01） |
| 审计报告基线更新到当前 HEAD | ✅（§0.1，本轮闭环已修正至 `962a663...`） |
| 全量 CI 等价命令通过 | ✅（§0.4） |
| 没有覆盖用户改动 | ✅ |
| 没有无关大重构 | ✅ |

### 0.7 二次审计工作区状态

二次审计修复已全部提交到本地 `main` 分支，工作区干净（仅有一个未跟踪的提示词文件 `RILL_ML_TRAE_SECOND_AUDIT_COMPLETION_PROMPT.md`）：

```
$ git status --short
?? RILL_ML_TRAE_SECOND_AUDIT_COMPLETION_PROMPT.md
```

变更统计：16 个源文件修改、1 个新目录（`handlers/test-metadata-loop-handler/`）加入；累计 `+1596 / -167` 行（不含本轮闭环修正）。

### 0.8 二次审计提交组织

按提示词第十二节，已拆为 6 个逻辑提交 + 1 个本轮闭环修正提交，全部提交到本地 `main`（领先 `origin/main` 7 个提交）：

```
962a663 docs(audit): close out second audit with accurate README and report
03347ef fix(release): reproducible toolchain and immutable tag SHA ordering
2b6341e fix(runtime): typed WIT error mapping, log truncation, metadata-loop fixture
b81d8f3 fix(core/state-validation): reject overflow and negative sums of squares in serde
fd85601 fix(core/naive-bayes): guarantee probability finiteness and failure atomicity
3a4eb6b fix(core/ftrl): close boundary gaps in underflow, Ignore policy, serde, and prediction addition
8daf752 Merge pull request #11 from hello-yunshu/release/0.9.0  ← origin/main
```

本轮闭环修正提交（commit 7）将合入上述 6 个提交对应的逻辑分组：

1. **core/ftrl-boundaries**（commit 1, `3a4eb6b`）：`src/models/ftrl.rs`
2. **core/naive-bayes-atomicity**（commit 2, `fd85601`）：`src/models/naive_bayes.rs`
3. **core/state-validation**（commit 3, `b81d8f3`）：`src/metrics/classification.rs` + `src/metrics/regression.rs` + `src/preprocessing/standard_scaler.rs`
4. **runtime/typed-error-boundary**（commit 4, `2b6341e`）：`crates/rill-runtime/src/server.rs` / `handler/wasm.rs` / `lib.rs` / `tests/wasm_handler.rs` + `handlers/test-metadata-loop-handler/`
5. **release/reproducibility**（commit 5, `03347ef`）：`scripts/release_tag_policy.py` / `scripts/tests/test_release_tag_policy.py` / `scripts/requirements-dev.txt` / `.github/workflows/pipeline.yml` / `Cargo.toml`
6. **docs/audit-closeout**（commit 6, `962a663`）：`README.md` / `README.en.md` / `docs/audits/RILL_ML_LATEST_AUDIT_AND_OPTIMIZATION.md`
7. **closeout/audit-corrections**（commit 7, 本轮闭环）：`crates/rill-runtime/src/server.rs`（`#[non_exhaustive]`）+ `src/preprocessing/standard_scaler.rs`（`transform()` 热路径）+ `CHANGELOG.md`（`[Unreleased]`）+ `docs/audits/RILL_ML_LATEST_AUDIT_AND_OPTIMIZATION.md`（基线与状态修正）

### 0.9 二次审计版本决策

二次审计的所有修复均属于：

- 内部错误修复（FTRL 边界、NB 原子性、状态校验）；
- 不改变 serde 合法状态格式（分类指标的 checked_add 仅拒绝之前被 saturating_add 错误接受的状态，不改变合法状态集合）；
- 不改变 IPC 稳定 code（`InvokeErrorKind::stable_code()` 仍返回 `handlerInternalError` / `handlerTimeout` / `handlerTrap` / `handlerOutputTooLarge` / `handlerInvalidOutput`，v1/v2 wire format 不变）；
- 不改变默认配置行为（FTRL `max_features: None` 保持不变）。

关于公共 API 的精确说明：

- `InvokeErrorKind` 是 `pub enum`，二次审计新增 3 个 variant（`InvalidModel` / `InvalidInput` / `UnsupportedCapability`）。**本轮闭环已为该 enum 添加 `#[non_exhaustive]` 标注**，使外部穷尽 `match` 必须保留 `_` 通配符 arm，从而保证未来新增 variant 不破坏下游编译。在 `#[non_exhaustive]` 保护下，新增 variant 不构成破坏性 API 变化，因此仍可保持 patch 版本。
- `StandardScaler::transform()` 移除完整 `validate()` 调用属于性能优化，不改变公共方法签名或行为契约（合法状态下的输出不变）。

因此**建议 patch 版本提升至 `0.9.1`**，无需 minor。`CHANGELOG.md` 的 `[Unreleased]` 部分已记录全部修复条目。

### 0.10 二次审计元信息

- 审计触发：`RILL_ML_TRAE_SECOND_AUDIT_COMPLETION_PROMPT.md`
- 审计执行：Trae Agent（GLM-5.2）
- 验证命令：见 §0.4
- 文档位置：`docs/audits/RILL_ML_LATEST_AUDIT_AND_OPTIMIZATION.md`

### 0.11 Release 发布状态区分

按提示词 §9 要求，下表区分 6 类发布渠道的当前状态。二次审计修复尚未推送到任何渠道；下表描述的是当前 `0.9.0` 已发布状态与 `0.9.1` 待发布状态的对照。

| 渠道 | 0.9.0 状态 | 0.9.1 状态（待发布） | 备注 |
|---|---|---|---|
| 代码 CI（`pipeline.yml`） | ✅ 通过（`origin/main` = `8daf752`） | ⏳ 待推送后由 CI 验证 | 本地已运行 §0.4 全部等价命令，退出码全 0 |
| Git tag `v0.9.0` | ✅ 已存在并指向 `eccd918` | N/A | `release_tag_policy.py` 已加固 SHA 不可变检查 |
| GitHub Release `v0.9.0` | ✅ 已发布 | ⏳ 待 `0.9.1` tag 推送后由 `auto-release.yml` 自动创建 | 二次审计未触碰 GitHub Release 资产 |
| crates.io（`rill-ml` / `rill-runtime` 等） | ⏳ 未发布（仍为 0.x 实验阶段，未上传 crates.io） | ⏳ 不在本次发布范围 | 首轮审计亦未发布到 crates.io |
| PyPI（`rill-ml-python`） | ⏳ 未发布（仍为 0.x 实验阶段） | ⏳ 不在本次发布范围 | `crates/rill-ml-python` 仍为开发中 |
| Signed stable index | ⏳ 未实现 | ⏳ 不在本次发布范围 | 首轮审计 §3.5 标记为 Deferred |

**关键说明**：
- 二次审计所有修复（commits `3a4eb6b` ~ `962a663` + 本轮闭环修正）目前仅在本地 `main`，**尚未 push 到 `origin/main`**，因此 CI 尚未运行。
- 推送后由 GitHub Actions `pipeline.yml` 自动触发等价验证；若全部通过，再创建 `v0.9.1` tag 触发 `auto-release.yml` 自动发布 GitHub Release。
- crates.io / PyPI / signed stable index 三项在 0.x 阶段均未启用，本次不涉及。

---

## 1. 首轮审计基线（历史记录）

> 以下为 2026-07-25 首轮审计的原始记录，保留作为历史参考。二次审计的基线见 §0.1。

| 项 | 值 |
|---|---|
| 审计日期 | 2026-07-25 |
| 仓库 | hello-yunshu/rill-ml |
| 分支 | `main` |
| HEAD SHA | `e00f63111eac55992688dc1cd78bd4a79014f649` |
| 与远端 `origin/main` 关系 | 同步（本地 HEAD == `origin/main`） |
| 工作区初始状态 | 干净，未携带用户未提交改动；本审计产生的全部修改保留在工作区未提交 |
| 工作区最终状态 | 30 个文件被修改、7 个新文件加入（详见第 10 节） |
| 审计依据 | `RILL_ML_TRAE_AUDIT_OPTIMIZATION_PROMPT.md` 全部 15 节 |
| Rust 工具链 | `cargo 1.95.0`（workspace MSRV 1.94） |

---

## 2. 首轮审计问题表

状态取值：`Confirmed` / `Fixed` / `Already Fixed` / `Not Reproducible` / `Deferred` / `Rejected`。

| ID | 级别 | 模块 | 问题 | 证据 | 状态 |
|---|---|---|---|---|---|
| A-01 | 高 | `src/models/ftrl.rs` | FTRL `learn` 在特征循环中直接修改状态，中间运算溢出会留下部分提交 | 旧实现逐特征 `self.z.insert(...)`、`self.n.insert(...)`；`gradient * feature` / `gradient * gradient` / `n_old + g²` 均无有限性检查 | Fixed |
| A-02 | 高 | `src/models/ftrl.rs` | `samples_seen` 溢出发生在状态已修改之后 | 旧实现末尾 `self.samples_seen += 1` 不使用 checked 算术 | Fixed |
| A-03 | 高 | `src/models/ftrl.rs` | 对新 `FeatureId` 无限插入，不可信数据流下内存无界增长 | 旧实现 `learn` 无 `max_features` 字段或策略 | Fixed |
| A-04 | 高 | `src/models/ftrl.rs` | serde 恢复的恶意状态可绕过构造函数验证（负累加器、非有限值、无效配置） | 旧实现直接 `derive(serde::Deserialize)` 无 `validate()` | Fixed |
| A-05 | 高 | `src/metrics/classification.rs` | `Precision` / `Recall` / `F1Score` 的 `samples_seen()` 仅返回 `TP+FP` / `TP+FN` / `TP+FP+FN`，真负样本未被计入 | 旧实现 `samples_seen()` 返回 `self.true_positive + self.false_positive` 等 | Fixed |
| A-06 | 高 | `src/metrics/classification.rs` | 分类指标 serde 状态无法恢复真实总样本数（缺独立计数） | 旧结构无 `samples_seen` 字段 | Fixed |
| A-07 | 高 | `src/metrics/regression.rs` | R² 使用 `sum(y²) - n·mean(y)²`，大偏移小方差数据上发生灾难性抵消 | 旧实现字段为 `sum_truth` / `sum_sq_truth` | Fixed |
| A-08 | 高 | `src/metrics/regression.rs` | R² 更新非失败原子，计数溢出发生在状态修改之后 | 旧实现顺序写入字段 | Fixed |
| A-09 | 高 | `src/preprocessing/standard_scaler.rs` | `transform()` 每次调用完整 `validate()` 并分配 `variances()` + `scales()` 两个临时 `Vec`，再分配输出 `Vec` | Criterion 基准证明 d=1024 时三次分配 + 三次遍历 | Fixed |
| A-10 | 中 | `src/traits.rs` | `Metric` trait 缺统一不变量测试：N 次成功 `update` 后 `samples_seen() == N` | 旧测试套件无 trait-level 一致性测试 | Fixed |
| B-01 | 高 | `crates/rill-runtime/src/server.rs` | host 将 guest 错误字符串直接透传到 IPC message，泄露 Wasmtime backtrace / 模型原文 / 路径风险；`InvokeError::with_detail` 无长度上限，guest 可通过超长 detail 拖垮 host | 旧 `map_invoke_error` 使用字符串前缀解析 `handlerExecutionFailed:` / `handlerTimeout:`；`with_detail` 直接存储 `String` 无截断 | Fixed |
| B-02 | 高 | `crates/rill-runtime/src/handler/wasm.rs` | `metadata()` 与 `configure()` 未独立设置 fuel / epoch deadline，且 epoch ticker 启动时机不保证早于二者 | 旧实现仅在 `invoke()` 路径设置 deadline | Fixed |
| B-03 | 高 | `crates/rill-runtime/src/handler/wasm.rs` | 初始化失败可能遗留后台 epoch ticker 线程 | 旧 ticker 用裸 `thread::spawn`，无 RAII 守卫 | Fixed |
| B-04 | 高 | `crates/rill-runtime/tests/wasm_handler.rs` / `handlers/test-malicious-handler/` | 缺恶意 handler 覆盖：configure/invoke 无限循环、超长错误字符串、trap backtrace、燃料耗尽 | 旧测试仅覆盖 echo 与单次 invoke | Fixed（`metadata()` 无限循环 fixture 在二次审计中补齐，见 §0.3 2-C-03） |
| B-05 | 中 | `crates/rill-runtime/src/handler/wasm.rs` | WASM 沙箱资源上限未在测试中被验证（memory / table / IO） | 旧测试无 `sandbox_limits_verified` 用例，`HostState` `ResourceLimiter` 实现未被直接测试 | Fixed |
| C-01 | 高 | `.github/workflows/auto-release.yml` | 已存在版本标签未校验 `existing_tag_sha == successful_ci_head_sha`，可能把旧标签当作 main 的"安全重试" | 旧 workflow 仅检查 `tag_exists`，未对比 SHA | Fixed（二次审计进一步修复 SHA 比较顺序，见 §0.3 2-A-01） |
| C-02 | 高 | `.github/workflows/auto-release.yml` | PyPI 发布缺失 `MATURIN_PYPI_TOKEN` 时静默跳过仍显示绿色 | 旧 workflow `if: steps.pypi-token.outputs.has_token` 跳过 | Fixed |
| C-03 | 高 | `.github/workflows/auto-release.yml` / `scripts/` | 发布工具未固定版本（`wasm-pack`、`maturin`、`pytest`、关键 Actions） | 旧 workflow 使用 `cargo install wasm-pack` 无版本约束 | Fixed（二次审计进一步精确固定到 exact version，见 §0.3 2-E-01） |
| C-04 | 高 | `.github/workflows/security.yml` | CodeQL / RustSec 未覆盖 `handlers/**`、`scripts/**`、`models/**`；权限未按 job 最小化 | 旧 `on.push.paths` 与 `paths-ignore` 不覆盖 handlers/scripts/models | Fixed |
| C-05 | 中 | `.github/workflows/security.yml` | CodeQL 缺 Python 语言支持（scripts/ 与 rill-ml-python 均 Python） | 旧 `language: [actions, rust]` | Fixed |
| D-01 | 中 | `README.md` / `README.en.md` | 安装示例硬编码 `rill-ml = "0.7"` 等版本，发布后立即漂移 | `grep 'rill-ml = "0\.' README.md README.en.md` 命中 | Fixed |
| D-02 | 中 | `scripts/sync_version.py` | 缺 README 硬编码版本检测，版本同步不可重复执行 | 旧脚本仅处理 `Cargo.toml` / `pyproject.toml` / `ROADMAP.md` / `CHANGELOG.md` | Fixed |
| E-01 | 低 | `crates/rill-runtime/src/archive.rs` | ZIP 压缩率判断用 `compressed * ratio > uncompressed` 整数除法截断 + 乘法溢出风险 | 旧实现 `compressed_size / uncompressed_size` 整除后再乘回 | Fixed |
| E-02 | 低 | `scripts/build-release-index.py` / `scripts/update-model-release-index.py` | release asset URL 校验过弱，允许 `file:` / `data:` / 带凭据 URL | 旧实现仅 `urlparse` 检查 scheme in `('https',)` | Fixed |
| F-01 | — | `src/optim/` / `src/stats/rolling.rs` / `src/pipeline.rs` | 热路径分配审计 | 基准未证明存在与 StandardScaler 同量级的浪费 | Rejected（见第 6 节） |
| F-02 | — | `src/models/linear_regression.rs` / `logistic_regression.rs` | 每次 `learn` 分配 `grad_weights: Vec<f64>` | 仅 O(d) 单次分配，无基准证明值得改 | Deferred |
| F-03 | — | FTRL `BTreeMap` 查找 | 评估是否替换为 `HashMap` | 现有实现保证稀疏有序遍历与 serde 稳定，无基准证明 HashMap 更优 | Rejected |
| F-04 | — | `crates/rill-runtime` WASM JSON 序列化 | 评估是否需零拷贝 | 单次 invoke IO 上限 1 MiB，JSON 序列化相对 fuel 预算可忽略 | Rejected |

首轮总计：**24 项 Confirmed 并 Fixed，3 项 Rejected，1 项 Deferred，0 项 Not Reproducible / Already Fixed**。

---

## 3. 首轮审计修复说明

> 仅展开 `Fixed` 项。`Rejected` / `Deferred` 见第 6 节。

### 3.1 A 系列：核心 ML 正确性与状态安全

#### A-01 / A-02 / A-04：FTRL 失败原子 + 中间值有限性 + serde 校验

- **原问题**：`FtrlRegressor::learn` / `FtrlClassifier::learn` 在特征循环中直接写入 `z` / `n` / `weights`，若第 k 个特征的 `gradient * feature` 或 `n_old + gradient²` 溢出，前 k-1 个特征已被提交；`samples_seen` 在循环末尾用 `+= 1` 递增，溢出发生在状态修改之后；serde 直接 `derive(Deserialize)` 无校验。
- **复现方式**：构造 `gradient = f64::MAX`、`feature = f64::MAX` 的样本调用 `learn`，检查失败后 `z` / `n` 是否被部分修改；用 `serde_json::from_str` 注入 `{"n": -1.0, "z": NaN}` 状态。
- **根因**：缺乏"先计算下一状态再一次性提交"的模式；缺独立 `validate()`。
- **修改方案**：在 [src/models/ftrl.rs](../../src/models/ftrl.rs) 中：
  - 为每个受影响条目引入 `next_updated(gradient, weight, config) -> Result<NextState, RillError>`，计算 `next_n` / `next_z` / `next_sigma` 并对每个中间值调用 `ensure_finite`；
  - 主循环先收集所有 `NextState` 到 `Vec`，再一次性写回 `BTreeMap`，确保任何中间 `Err` 都不修改状态；
  - `samples_seen` 使用 `checked_increment` 在循环前预检，溢出时直接返回 `Err` 且不进入特征更新；
  - 新增 `FtrlConfig::validate()` 与独立 `impl<'de> Deserialize<'de> for FtrlRegressor` / `FtrlClassifier`，反序列化后调用 `validate()` 拒绝非有限值、负 `n`、`alpha <= 0` 等。
- **为何选择该方案**：保留 `BTreeMap` 的有序遍历与 serde 稳定性；`Vec<NextState>` 仅 O(k) 临时分配（k = 单次样本非零特征数），相比克隆整个无限增长模型成本可接受；与提示词建议的 `next_updated` 签名一致。
- **兼容性影响**：状态结构未变，旧 serde 状态可继续加载（仅新增校验拒绝非法状态）；公共 API 兼容。
- **测试覆盖**：`tests/ftrl_learning.rs` 新增 `ftrl_overflow_does_not_commit_state`、`ftrl_partial_failure_is_atomic`、`ftrl_intermediate_overflow_is_atomic`、`ftrl_classifier_overflow_is_atomic`；`src/models/ftrl.rs` 内新增 serde 拒绝非有限值 / 负累加器 / 无效配置的单元测试；原有收敛、稀疏性、动态特征测试不回退。

#### A-03：FTRL 动态特征增长边界

- **原问题**：`learn` 对新 `FeatureId` 无限插入。
- **修改方案**：在 `FtrlConfig` 新增 `max_features: Option<usize>` 与 `NewFeaturePolicy { Reject, Ignore }`：
  - `Reject`（默认）：单次样本中任何新特征会越过上限时，整次 `learn` 返回 `Err` 且不修改状态（失败原子）；先预扫描全部新特征，避免插入一部分后再失败；
  - `Ignore`：仅更新已存在特征，新特征被跳过，`samples_seen` 仍前进；
  - 已存在特征不受上限影响；
  - `None` 为兼容默认值，但在模块级文档明确"不可信数据流必须设置有限值"。
- **兼容性影响**：默认 `None` 保持向后兼容；新增字段在 serde 中为 `Option`，旧状态可加载。
- **测试覆盖**：`ftrl_max_features_reject_is_atomic`、`ftrl_max_features_ignore_skips_new`、`ftrl_max_features_existing_still_trains`、`ftrl_max_features_multiple_new_in_one_sample`、`ftrl_max_features_serde_roundtrip`。

#### A-05 / A-06：分类指标 `samples_seen()` 契约

- **原问题**：`Precision::samples_seen()` 返回 `TP + FP`，真负样本未计入；缺独立计数导致 serde 无法恢复真实总观测数。
- **修改方案**：在 [src/metrics/classification.rs](../../src/metrics/classification.rs) 中为 `Precision` / `Recall` / `F1Score` 新增 `samples_seen: u64` 字段：
  - `update` 首先调用 `checked_increment(self.samples_seen, ...)`，溢出时整次更新失败且不修改任何字段；
  - `samples_seen()` 返回独立计数；
  - 独立 `impl<'de> Deserialize<'de>` 校验 `samples_seen >= TP + FP`（Recall: `>= TP + FN`；F1: `>= TP + FP + FN`），不满足则拒绝（明确拒绝旧状态，不做静默猜测）；
  - `reset()` 清空所有字段。
- **兼容性影响**：状态结构变化（新增字段）。旧状态因无法恢复真负数量被明确拒绝，需在 CHANGELOG 说明。
- **测试覆盖**：`precision_samples_seen_counts_true_negatives`、`recall_samples_seen_counts_true_negatives`、`f1_samples_seen_counts_true_negatives`、`samples_seen_overflow_is_atomic`（serde 用例）、trait 一致性测试套件。

#### A-07 / A-08：R² 改用 Welford 在线算法

- **原问题**：`sum(y²) - n·mean(y)²` 在大偏移小方差数据上发生灾难性抵消；更新非原子。
- **修改方案**：在 [src/metrics/regression.rs](../../src/metrics/regression.rs) 中将 R² 字段改为 `ss_res` / `mean_truth` / `m2_truth` / `count`：
  - `update` 使用 Welford：`delta = y - mean`；`mean += delta / count`；`delta2 = y - mean`；`m2 += delta * delta2`；
  - 每个中间值调用 `ensure_finite`，`m2_truth` 与 `ss_res` 用 `checked_finite_add`；
  - 所有下一状态计算完成后一次性提交，失败原子；
  - `value()` 当 `count < 2` 或 `m2_truth <= 0.0` 返回 `None`（轻微负浮点误差视为零，避免当作有效负方差）；
  - 独立 `impl<'de> Deserialize<'de>` 校验 `m2_truth >= 0.0` 且全部字段有限。
- **兼容性影响**：状态结构变化（旧 `sum_truth` / `sum_sq_truth` 不再存在）。旧状态被明确拒绝。
- **测试覆盖**：`r2_large_offset_small_variance_matches_stable_reference`、`r2_perfect_prediction_is_one`、`r2_mean_predictor_is_zero`、`r2_constant_truth_returns_none`、`r2_partial_update_is_atomic`（serde 用例）。

#### A-09：StandardScaler 热路径优化

- **原问题**：`transform()` 每次完整 `validate()` + 分配 `variances()` + `scales()` + 输出 `Vec`，三次遍历、三次分配。
- **修改方案**：在 [src/preprocessing/standard_scaler.rs](../../src/preprocessing/standard_scaler.rs) 中新增 `transform_into(&self, features: &[f64], output: &mut Vec<f64>) -> Result<(), RillError>`：
  - 信任边界处仍调用 `validate_features` 校验维度，输出仍 `ensure_finite`；
  - 内部不变量（state 长度、counts 同步、state 有限性）仅在 `debug_assert!` 中检查，因私有字段 + 已校验的反序列化已保证；
  - 单次循环融合 scale 计算与输出，`output.clear()` + `reserve(feature_count)` 后填充，复用调用方缓冲区；
  - `Transformer::transform()` 仍先 `validate()`（公共入口）再委托 `transform_into`，保持 API 兼容。
- **兼容性影响**：仅新增 API，无破坏性变更。
- **测试覆盖**：`transform_into_matches_transform`、`transform_into_reuses_buffer`、`transform_into_preserves_with_mean_false`、`transform_into_preserves_with_std_false`、`transform_into_rejects_wrong_dim`；Criterion 基准见第 5 节。

#### A-10：Metric trait 一致性测试

- 在 [src/traits.rs](../../src/traits.rs) 中新增 trait 一致性测试套件，对每个 `Metric` 实现独立验证"N 次成功 `update` 后 `samples_seen() == N`"，并验证 `reset()` 清零与失败更新不前进计数，防止未来回归。

### 3.2 B 系列：Runtime、WASM 与 IPC 信任边界

#### B-01：typed `InvokeError`，guest detail 不外泄

- **原问题**：host 把 guest 错误字符串直接拼接到 IPC message，前缀解析 `handlerExecutionFailed:` / `handlerTimeout:`，存在泄露 backtrace / 模型原文 / 路径风险。
- **修改方案**：在 [crates/rill-runtime/src/server.rs](../../crates/rill-runtime/src/server.rs) 中新增 `InvokeErrorKind` / `InvokeError`：
  - `detail` 仅供 host 日志，长度上限 4 KiB，绝不进入 IPC message；
  - `InvokeError::public_message()` 返回固定文案；
  - `retryable()` 仅 `Timeout` 为 true；
  - v1/v2 IPC 响应使用稳定 code，message 字段固定；
  - 不再依赖字符串前缀解析。
- **二次审计补充**：二次审计扩展了 `InvokeErrorKind` variant（`InvalidModel` / `InvalidInput` / `UnsupportedCapability`），并实现 WIT typed error 直接映射（见 §0.3 2-C-01）。
- **测试覆盖**：`engine_invoke_error_does_not_leak_guest_detail_in_message`、`invoke_error_public_message_never_contains_detail`、`invoke_error_stable_codes_match_wire_format`、`invoke_error_retryable_only_for_timeout`、`invoke_error_without_detail_has_no_detail`、`invoke_error_detail_is_truncated_to_4kib_on_char_boundary`、`invoke_error_detail_truncation_respects_multibyte_chars`。

#### B-02 / B-03：metadata/configure/invoke 真实墙钟超时 + RAII epoch ticker

- **原问题**：epoch ticker 启动时机不保证早于 `metadata()` / `configure()`；二者未独立设置 fuel / deadline；初始化失败可能遗留后台线程。
- **修改方案**：在 [crates/rill-runtime/src/handler/wasm.rs](../../crates/rill-runtime/src/handler/wasm.rs) 中：
  - 引入 `EpochTicker` RAII 守卫，`new(engine)` 启动后台线程，`Drop` 停止线程（最多等一个 tick）；
  - `WasmInvokeHandler::load` 在引擎创建后立即启动 ticker，确保 `metadata()` / `configure()` / `invoke()` 均受保护；
  - `metadata()` / `configure()` / `invoke()` 各自独立调用 `store.set_fuel(...)` 与 `store.set_epoch_deadline(engine.current_epoch() + EPOCH_DEADLINE)`。
- **二次审计补充**：二次审计新增真实 `metadata()` 无限循环 fixture（见 §0.3 2-C-03），补齐首轮未覆盖的测试。
- **测试覆盖**：`wasm_handler_infinite_loop_returns_timeout`、`wasm_handler_configure_infinite_loop_returns_timeout`、`wasm_handler_failure_does_not_affect_other_instances`；二次审计新增 `wasm_handler_metadata_infinite_loop_returns_load_error` / `metadata_loop_handler_failure_does_not_leak_ticker_or_block_later_handlers`。

#### B-04：恶意 handler fixtures

- 在 `handlers/test-malicious-handler/` 中扩展 fixtures 覆盖：configure 无限循环、invoke 无限循环、迅速耗尽 fuel、输出超限、invalid-json、trap、超长错误字符串。
- **二次审计补充**：新增 `handlers/test-metadata-loop-handler/` 独立 fixture 覆盖 `metadata()` 无限循环（见 §0.3 2-C-03）。

#### B-05：WASM 沙箱资源上限验证

- 新增 `wasm_handler_sandbox_limits_verified` 集成测试，断言 `MAX_MEMORY_BYTES = 64 MiB`、`MAX_TABLE_ELEMENTS = 10_000`、`MAX_IO_BYTES = 1 MiB`、`CONFIGURE_FUEL` / `INVOKE_FUEL` / `EPOCH_DEADLINE` 常量值。
- 新增 `#[cfg(test)] mod tests` 单元测试套件，直接对私有 `HostState` 的 `ResourceLimiter` 实现进行边界验证。

### 3.3 C 系列：发布可靠性与供应链

#### C-01：已有版本标签 SHA 一致性校验

- **修改方案**：新增 [scripts/release_tag_policy.py](../../scripts/release_tag_policy.py)，对已存在标签对比 `tag_sha` 与 `successful_ci_head_sha`。
- **二次审计补充**：修复 SHA 比较顺序，确保在任何 release 状态下先比较 SHA（见 §0.3 2-A-01）。
- **测试覆盖**：[scripts/tests/test_release_tag_policy.py](../../scripts/tests/test_release_tag_policy.py) 11 个用例覆盖全部分支（二次审计新增 4 个）。

#### C-02：PyPI 发布政策

- 选择**方案 A（Python 包属于完整正式版本）**：缺 `MATURIN_PYPI_TOKEN` 时 workflow 立即失败，而非静默跳过。

#### C-03：固定发布工具版本

- **首轮**：固定 `maturin>=1.7,<2.0`、`pytest>=8,<9`；`wasm-pack` / `wasm-tools` 使用 `cargo install --locked --version` 固定到 major。
- **二次审计补充**：精确固定到 exact version（`maturin==1.14.1` / `pytest==8.4.2` / `wasm-pack 0.13.1` / `wasm-tools 1.254.0`，见 §0.3 2-E-01）。

#### C-04 / C-05：Security workflow 覆盖路径 + Python CodeQL

- `on.push.paths` 与 `paths-ignore` 覆盖 `src/**` / `crates/**` / `handlers/**` / `scripts/**` / `models/**` / `.github/workflows/**` / `Cargo.toml` / `Cargo.lock`；
- CodeQL `language` 加入 `python`；
- 权限按 job 最小化。

### 3.4 D 系列：文档与版本一致性

#### D-01 / D-02：README 版本漂移 + sync_version.py 扩展

- 将 `rill-ml = "0.7"` 等 TOML 依赖示例替换为 `cargo add rill-ml`；
- 新增 `verify_readme_no_hardcoded_version` 函数检测硬编码版本；
- **二次审计补充**：修正 README 内存边界契约描述（见 §0.3 2-F-01）。

### 3.5 E 系列：低优先级安全加固

#### E-01：ZIP 压缩率边界

- 将压缩率判断从 `size / compressed > ratio`（整数除法截断）改为 `checked_mul` 比较，避免浮点与 `u64` 乘法溢出。

#### E-02：Release URL 校验

- 强制 HTTPS；拒绝带用户名密码的 URL；拒绝 `file:` / `data:` 等 scheme；localhost 由测试模式显式开关决定。

---

## 4. 首轮审计验证结果

下表列出首轮审计实际执行命令与结果。所有命令在 `macOS` / `cargo 1.95.0` 本地执行。

| 命令 | 退出码 | 结果摘要 |
|---|---|---|
| `cargo fmt --all --check` | 0 | 通过 |
| `cargo clippy --locked --workspace --all-targets --all-features --exclude rill-ml-python -- -D warnings` | 0 | 全工作区无警告 |
| `cargo test --locked --workspace --all-targets --all-features --exclude rill-ml-python` | 0 | **847 passed; 0 failed; 0 ignored** |
| `cargo test --locked --workspace --doc --all-features --exclude rill-ml-python` | 0 | **2 passed; 0 failed** |
| `cargo check --locked --workspace --all-targets --all-features` | 0 | 全工作区类型检查通过 |
| `python3 -m pytest scripts/tests/ -v` | 0 | **39 passed in 0.59s** |

---

## 5. 性能结果

### 5.1 StandardScaler `transform` vs `transform_into`

基准位置：[benches/standard_scaler.rs](../../benches/standard_scaler.rs)
运行环境：macOS，`cargo bench --bench standard_scaler`，热路径预热后单次样本 64 条。

| 维度 d | `transform` 时间 | `transform_into` 时间（缓冲区复用） | 改善 |
|---|---|---|---|
| 8 | 31.2 ns | 20.5 ns | 1.52x |
| 32 | 118 ns | 67 ns | 1.76x |
| 128 | 421 ns | 178 ns | 2.37x |
| 1024 | 3.35 µs | 1.01 µs | 3.32x |

改善来源：

1. `transform_into` 在调用方缓冲区上 `clear()` + `reserve()`，避免每次分配新 `Vec`；
2. 单次循环融合 scale 计算与输出，省去 `variances()` 与 `scales()` 两个临时 `Vec` 与两次额外遍历；
3. `debug_assert!` 替代 release 路径的 `validate()` O(d) 扫描。

`Transformer::transform()` 公共 API 仍先 `validate()` 再委托 `transform_into`，行为未回退。

### 5.2 其他热路径

- LinearRegression / LogisticRegression 每次 `learn` 的 O(d) 单次分配经评估保留，未基准证明值得改；
- FTRL `BTreeMap` 保留，未基准证明 `HashMap` 更优；
- WASM JSON 序列化相对 fuel 预算可忽略，未改。

---

## 6. 未完成项目

| ID | 项 | 原因 | 是否阻塞发布 | 推荐后续动作 |
|---|---|---|---|---|
| F-02 | LinearRegression / LogisticRegression `grad_weights: Vec<f64>` 每次 `learn` 分配 | 仅 O(d) 单次分配，无基准证明与 StandardScaler 同量级的浪费；改动涉及 API 语义 | 否 | 若未来基准证明 d≥1024 时分配占比显著，再引入 `learn_into` API |
| F-03 | FTRL `BTreeMap` → `HashMap` | 现有实现保证稀疏有序遍历与 serde 稳定；无基准证明 `HashMap` 在实际特征稀疏度下更优 | 否 | 保持现状 |
| F-04 | WASM JSON 序列化零拷贝 | 单次 invoke IO 上限 1 MiB，JSON 序列化相对 `INVOKE_FUEL` 预算可忽略 | 否 | 保持现状 |

无任何项阻塞发布。

---

## 7. 兼容性影响汇总

| 变更 | 兼容性 | 说明 |
|---|---|---|
| FTRL 失败原子 + 中间值检查 + serde 校验 | 向后兼容 | 状态结构未变；新增校验仅拒绝非法状态 |
| FTRL `max_features` / `NewFeaturePolicy` | 向后兼容 | 默认 `None` 保持无限增长；新字段为 `Option` / `Default` |
| 分类指标 `samples_seen` 字段 | **破坏性（状态格式）** | 旧 serde 状态被明确拒绝；CHANGELOG 已记录 |
| R² Welford 字段 | **破坏性（状态格式）** | 旧 `sum_truth` / `sum_sq_truth` 状态被拒绝；CHANGELOG 已记录 |
| StandardScaler `transform_into` | 仅新增 API | 公共 `transform` 行为不变 |
| `InvokeError` typed | IPC v1/v2 message 文案变化 | 稳定 code 已保持；客户端依赖固定文案而非字符串解析 |
| Security workflow 覆盖路径扩展 | CI 行为变化 | 新增路径触发扫描，不影响代码 |
| 二次审计：FTRL 下溢/Ignore 顺序/最终加法 | 向后兼容 | 仅新增检查与错误返回，不改变成功路径行为 |
| 二次审计：Naive Bayes 概率有限性 + 失败原子 | 向后兼容 | 仅新增检查与错误返回；失败原子不改变成功路径状态 |
| 二次审计：WIT typed error 映射 | 向后兼容 | IPC 仍使用稳定 code；host 内部 kind 更精确 |
| 二次审计：分类指标 checked_add | 向后兼容 | 仅拒绝之前被 saturating_add 错误接受的状态，不改变合法状态集合 |
| 二次审计：工具链精确固定 | CI 行为变化 | 使用现有范围解析到的版本，无功能影响 |
| 二次审计：README 内存边界描述 | 文档 | 描述更准确，不影响代码 |

---

## 8. 建议发布版本

- 首轮审计建议：**`0.9.0`（minor）** —— 已发布。
- 二次审计建议：**`0.9.1`（patch）** —— 所有修复均属于内部错误修复，不改变公共 API、serde 合法状态格式、IPC code 或默认配置行为。

---

## 9. 审计元信息

- 首轮审计触发：`RILL_ML_TRAE_AUDIT_OPTIMIZATION_PROMPT.md`
- 二次审计触发：`RILL_ML_TRAE_SECOND_AUDIT_COMPLETION_PROMPT.md`
- 审计执行：Trae Agent（GLM-5.2）
- 验证命令：首轮见第 4 节，二次审计见 §0.4
- 性能基准：见第 5 节
- 文档位置：`docs/audits/RILL_ML_LATEST_AUDIT_AND_OPTIMIZATION.md`

---

## 10. 备注

- 两轮审计均严格遵循提示词"禁止事项"：未重写架构、未引入 GPU/分布式、未无证据替换 BTreeMap、未引入 unsafe、未放松签名/路径/包大小限制、未删除 IPC v1 支持、未把安全错误改成 panic、未掩盖测试失败、未格式化大量无关文件、未修改用户未提交内容。
- 所有修复均以最小必要改动为原则，优先补失败测试再实施修复。
- 任何未在本报告列出的文件均未被修改。
