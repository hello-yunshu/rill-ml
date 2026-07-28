# RillML 1.0 冻结最终收尾与 `1.0.0-rc.6` 发布提示词

你将继续处理 GitHub 仓库：

- 仓库：`https://github.com/hello-yunshu/rill-ml`
- 默认分支：`main`
- 当前已发布候选版本：`1.0.0-rc.5`
- `v1.0.0-rc.5` 代码 SHA：`4690b4adb81bb6db7b7ca7b7a37138975a03bd06`
- 上一次已知 `main` 审计文档 HEAD：`5df107c05fb3ca86f3ec8618680d1937d444b75f`
- 本阶段目标：完成 1.0 冻结审计剩余的最小闭环，并发布不可变的 `1.0.0-rc.6`
- 本阶段仍禁止直接发布最终 `1.0.0`

开始前必须重新拉取远端最新代码、标签和 Actions 状态。不得假设上述 SHA 仍是最新状态。

---

# 一、永久发布规则：macOS 不得再因 Apple 证书缺失被阻断

这是明确、长期且不可回退的项目决策：

> 项目现在没有 Apple Developer ID Application 证书，未来也不会配置。任何 RC、正式版或普通发布都不得因此失败、跳过 macOS 资产或被判定为不满足发布条件。

必须将现有行为从：

```text
没有 Apple 签名 Secret
→ 跳过 macOS 编译
→ 不生成 macOS 资产
```

改为：

```text
没有 Apple 签名 Secret
→ 正常编译 macOS aarch64
→ 跳过 codesign 和 notarization
→ 上传未签名 macOS 二进制
→ 在文件名、release notes、index metadata 或文档中明确标注 unsigned / not notarized
→ 继续完成整个 Release
```

## 1.1 强制要求

- macOS aarch64 必须继续作为官方构建平台；
- 缺少 Apple Secret 时不得跳过编译；
- 不得要求用户配置 Apple Developer ID；
- 不得将“未签名”列为阻断项；
- 不得删除 macOS 支持声明；
- 不得伪装成已签名；
- 必须保留 SHA-256、release index 签名和资产完整性校验；
- Gatekeeper 提示、手工授权方式和风险说明应写入文档；
- macOS unsigned 是已接受的正式交付形态；
- 最终 `1.0.0` 也遵循同一规则。

## 1.2 推荐资产命名

可以保留：

```text
rill-runtime-1.0.0-rc.6-macos-aarch64
```

但 release notes 和 candidate index 必须明确：

```text
codeSigning: "unsigned"
notarization: false
```

如果现有 release-index schema 不适合增加字段，可以先在：

- artifact metadata；
- release notes；
- `STABILITY.md`；
- `README.md`；

中明确说明，不得为了增加字段破坏已冻结的旧 schema。

## 1.3 CI 行为

macOS job 应始终：

1. 安装 Rust；
2. 编译 `aarch64-apple-darwin`；
3. 若有签名 Secret，则可选执行 codesign；
4. 若无 Secret，打印明确 warning；
5. 继续 Stage asset；
6. 上传 artifact；
7. Release index 包含 macOS 资产；
8. 整个 job 成功。

不得再使用“没有 Secret 就跳过 build/stage/upload”的条件。

---

# 二、本轮只解决以下剩余问题

不要重新扩展算法，不要重新设计整个架构，不要重复已经完成的 WASM 沙箱、IPC、WIT 和 0.13.0 发布工作。

只处理：

1. 真正的 SemVer 兼容检查；
2. Stable serde 状态承诺与 fixture 覆盖一致；
3. 修复无效或无断言的恶意状态测试；
4. Security workflow 真实执行并记录证据；
5. 完成一次真实宿主 smoke test；
6. crates 发布列表由 `release-plan.toml` 驱动；
7. macOS unsigned 资产正常发布；
8. 修正最终审计报告中的过期或夸大表述；
9. 发布 `1.0.0-rc.6`；
10. 保持 `local-ai-stable` 仍指向 `0.13.0`。

---

# 三、开始前检查

执行：

```bash
git status --short
git branch --show-current
git fetch --prune --tags origin
git rev-parse HEAD
git rev-parse origin/main
git rev-parse v1.0.0-rc.5^{commit}
git log --oneline --decorate -30
git rev-list --left-right --count origin/main...HEAD
```

要求：

- 不覆盖用户本地修改；
- 不得 `reset --hard`；
- 不得 force push；
- 不得移动、删除或覆盖 `v1.0.0-rc.5`；
- 不得重发 `rc.5`；
- 修复后使用新的不可变版本：

```text
1.0.0-rc.6
```

建议分支：

```text
work/1.0-rc6-final-closeout
```

---

# 四、真正接入 `cargo-semver-checks`

当前 `cargo-public-api` 文本 baseline 可以保留，但它不能替代真正的 SemVer 分析。

## 4.1 必须增加

对四个 Stable crate：

```text
rill-ml
rill-runtime-protocol
rill-handler-api
rill-runtime
```

在 CI 中运行真正的：

```bash
cargo semver-checks
```

基线使用最近已发布的 RC：

```text
1.0.0-rc.5
```

或等价的已发布 crate baseline。

## 4.2 行为要求

- 删除公共 API → 失败；
- 更改签名 → 失败；
- 收紧 trait bound → 失败；
- 删除 enum variant → 失败；
- 删除 public field → 失败；
- 合法 additive API → 允许；
- 不能通过简单重新生成文本 baseline 掩盖 breaking change。

## 4.3 保留现有 baseline

`api-baseline/*.txt` 继续用于：

- 人工审阅；
- 发现意外 API 漂移；
- 文档证据。

但文档不得再写“完全相等比较允许 additive”。

修正为：

```text
cargo-public-api baseline 用于精确漂移审阅；
cargo-semver-checks 用于判断兼容性。
```

## 4.4 工具版本

在 CI 和审计报告中记录：

```text
cargo-public-api version
cargo-semver-checks version
nightly toolchain version
```

---

# 五、让 serde 状态承诺与真实覆盖一致

当前 `STABILITY.md` 对整个 `rill-ml` serde model state 的承诺过宽，而跨版本 fixture 只覆盖部分类型。

必须二选一，不得继续含糊。

## 5.1 推荐方案：状态稳定白名单

在 `STABILITY.md` 中明确列出 1.x 状态格式稳定的类型，例如当前已有完整 v0.13.0 + v1 fixture 的 26 类。

新增类似章节：

```text
Stable state schema types
Preview state schema types
```

要求：

- Rust API Stable 不等于所有 serde schema Stable；
- 未列入白名单的 serde 类型明确标为 Preview；
- Preview 类型仍可序列化，但不承诺跨 1.x 永久兼容；
- 文档、CHANGELOG、SECURITY 和 rustdoc 表述一致。

## 5.2 可选方案：补齐全部 fixture

如果决定整个 `rill-ml` serde 状态都 Stable，则必须为所有公开可序列化状态补充：

```text
tests/fixtures/state/v0.13.0/
tests/fixtures/state/v1/
```

至少检查：

- PageHinkley
- Adwin
- Kswin
- DriftAwareModel
- DriftEvent
- StaticStrategy
- TimeDecayedMean
- LearningRateScheduler
- FixedWindowBuffer
- LogisticRegression
- ExponentiallyWeightedMeanRegressor
- LastValueRegressor
- pipeline 类型
- 其他所有公开 serde 状态类型

不得只做当前代码 roundtrip。

## 5.3 自动覆盖检查

建议新增脚本或测试：

```text
scripts/check_state_fixture_coverage.py
```

通过明确 manifest 维护：

```toml
[[stable_state]]
type = "LinearRegression"
fixture = "linear_regression"

[[preview_state]]
type = "PageHinkley"
reason = "state schema remains preview"
```

CI 必须验证：

- Stable 状态都有 v0.13.0 fixture；
- Stable 状态都有 v1 fixture；
- Stable 状态实现 `ValidateState`；
- fixture 文件存在；
- manifest 无重复；
- 文档白名单与 manifest 一致。

---

# 六、修复无效恶意状态测试

当前有测试名声称“拒绝非法状态”，但实际没有断言。

## 6.1 必须删除或重写

例如：

```text
negative_variance_count_rejected
```

当前只是：

```rust
let _ = result;
```

这是无效测试。

必须改为真正违反明确不变量、且结果必须失败的案例：

```rust
assert!(result.is_err());
```

## 6.2 非有限值测试

JSON 无法直接表示 NaN/Inf，因此使用 `null` 只能证明 serde 拒绝类型错误，不能证明 `ValidateState` 工作。

需要增加真正触发 validator 的测试，可选择：

- 先构造合法对象，再通过内部 test helper 注入非有限值；
- 针对可反序列化为极大值后在计算中溢出的状态；
- 自定义测试类型实现 `ValidateState`；
- 单独测试 `validate_state()` 对非有限字段的拒绝。

要求测试名和实际证明内容一致。

## 6.3 最低负向测试

至少覆盖：

- 错误 format version；
- 超大 JSON；
- 维度不匹配；
- optimizer 参数数不匹配；
- 非法 learning rate；
- encoder 映射不一致；
- bandit arm 数不一致；
- drift buffer 超出容量；
- 非有限内部状态；
- 失败时不返回模型。

---

# 七、Security workflow 必须真实闭环

不得再写“没有触发是预期”。

本轮会修改：

```text
Cargo.toml
Cargo.lock
src/**
crates/**
scripts/**
.github/workflows/**
```

这些都应触发 Security workflow。

## 7.1 必须验证

- CodeQL static analysis；
- RustSec advisory audit；
- 对应 commit SHA；
- run ID；
- status；
- conclusion；
- 所有 job 和 step 结果。

## 7.2 如果没有自动触发

检查：

- paths filter；
- workflow syntax；
- branch；
- permissions；
- concurrency cancellation；
- Actions 是否被禁用。

必要时使用 `workflow_dispatch` 手动触发，但最终报告必须说明：

```text
自动触发 / 手动触发
对应 SHA
原因
结果
```

## 7.3 RC 准入

`rc.6` 发布前必须：

```text
CI = success
Security = success
Docs = success
```

---

# 八、真实宿主 smoke test

必须完成至少一个真实宿主集成验证。

## 8.1 优先选择

优先使用 Mira 相关宿主；若当前仓库无法直接集成，则建立一个独立、真实的 external-host smoke 工程。

不能只使用同仓库单元测试冒充真实宿主。

## 8.2 最低验证流程

宿主必须通过已发布 `rc.6` 产物或候选资产完成：

1. 下载或安装 runtime；
2. 下载 `candidate-index.json`；
3. 验证 index 签名；
4. 获取 `.rillpack`；
5. 获取 `.rillhandler`；
6. 启动 runtime；
7. 完成 IPC handshake；
8. health 成功；
9. invoke 成功；
10. 错误请求返回稳定 error code；
11. 正常退出；
12. 记录日志。

## 8.3 证据

审计报告记录：

- 宿主名称；
- 宿主仓库；
- 宿主 commit；
- 操作系统；
- 使用的 runtime 版本；
- 使用的 model/handler 版本；
- 完整命令；
- handshake 结果；
- invoke 结果；
- 日志位置；
- 测试结论。

如果 Mira 无法在当前环境运行，允许使用独立 external-host fixture repo，但必须是一个独立 crate/process，不能直接调用仓库内部 private API。

---

# 九、发布列表必须由 `release-plan.toml` 驱动

当前 Release workflow 仍硬编码发布 Stable 和 Preview crate。

必须改为：

```text
读取 release-plan.toml
→ 只发布 [stable].crates
```

## 9.1 要求

- RC 只发布 Stable group；
- Preview group 不因 RC tag 被重新发布；
- Python PyPI 继续保持 Preview 0.13.0；
- 不得依赖 crates.io 返回“already exists”来跳过 Preview；
- 发布顺序必须处理内部依赖；
- release-plan 解析失败 → Release 失败；
- Stable group 中 crate 缺失 → 失败；
- 未知 crate → 失败；
- 重复 crate → 失败。

## 9.2 测试

新增脚本测试：

- 正常读取；
- Stable 顺序；
- Preview 不发布；
- 重复项拒绝；
- 未知 crate 拒绝；
- 空 Stable 拒绝；
- prerelease version 与 release tag 一致。

---

# 十、macOS unsigned 发布实现

## 10.1 Workflow 修改

移除对 macOS build/stage/upload 的 Secret 条件。

类似：

```yaml
- name: Build runtime
  run: cargo build ...

- name: Code-sign macOS runtime
  if: matrix.target_os == 'macos' && secrets_are_present
  run: codesign ...

- name: Mark unsigned macOS asset
  if: matrix.target_os == 'macos' && secrets_are_missing
  run: echo "unsigned macOS build"

- name: Stage asset
  run: ...
```

## 10.2 产物元数据

在 candidate index 中准确表达：

- platform = macos；
- arch = aarch64；
- checksum；
- version；
- unsigned/not notarized 状态。

如果 schema 已冻结，不要修改旧字段语义。可以使用：

- 新 optional metadata；
- release notes；
- 附加 `.json` metadata 资产；
- 文档；

但必须保证旧客户端仍可读取。

## 10.3 用户说明

README 增加 macOS 使用说明：

```text
该二进制未经过 Apple Developer ID 签名和 notarization。
首次运行可能需要：
- Finder 中右键打开；
- 或在“系统设置 → 隐私与安全性”中允许；
- 或用户自行移除 quarantine 属性。
```

不要提供绕过企业安全策略的表述，只说明 macOS 标准用户授权流程。

## 10.4 测试

RC Release 必须实际包含：

```text
rill-runtime-1.0.0-rc.6-macos-aarch64
```

并验证：

```bash
file <asset>
shasum -a 256 <asset>
```

candidate index 中必须存在对应条目。

---

# 十一、修正审计报告

更新：

```text
docs/audits/RILL_ML_1_0_FREEZE_AUDIT.md
```

## 11.1 HEAD 字段

准确区分：

```text
rc.6 代码/tag HEAD
审计报告提交 HEAD
当前 origin/main HEAD
```

报告自身提交后不能继续声称文档 HEAD 等于上一个代码 SHA。

## 11.2 修正夸大表述

不得再写：

```text
check_runtime_public_api.py 证明所有内部模块都不可导入
```

应写：

```text
ticker 专项 compile-fail 脚本只证明 ticker probe 不可导入；
整体公共 API 由源码可见性、cargo-public-api baseline 和 cargo-semver-checks 共同约束。
```

不得再写：

```text
all serializable model types
```

除非确实全部覆盖；否则使用稳定状态白名单的准确表述。

## 11.3 Security

必须记录真实 Security run ID，不得写“未触发是预期”。

## 11.4 macOS

明确：

```text
macOS aarch64 official asset is unsigned and not notarized by permanent project policy.
This is not a release blocker.
```

## 11.5 最终准入结论

只有全部真实完成后才能写：

```text
1.0 freeze complete
1.0.0-rc.6 released
eligible for final 1.0.0 after RC observation
```

---

# 十二、版本与发布

## 12.1 版本

Stable group：

```text
1.0.0-rc.6
```

Preview group保持：

```text
0.13.0
```

同步：

- Cargo.toml；
- Cargo.lock；
- release-plan.toml；
- Stable crate Cargo.toml；
- model manifest；
- handler manifest；
- CHANGELOG；
- release workflow；
- fixture metadata（如需要）。

## 12.2 标签

创建新的不可变：

```text
v1.0.0-rc.6
```

不得移动：

```text
v1.0.0-rc.1
v1.0.0-rc.2
v1.0.0-rc.3
v1.0.0-rc.4
v1.0.0-rc.5
```

## 12.3 Candidate 发布

必须包含：

- Linux x86_64 runtime；
- Windows x86_64 runtime；
- macOS aarch64 unsigned runtime；
- `.rillpack`；
- `.rillhandler`；
- signed `candidate-index.json`；
- GitHub prerelease；
- Stable crates on crates.io；
- 不发布 Preview crate；
- 不发布 Python 1.0；
- 更新 `local-ai-candidate`；
- 不更新 `local-ai-stable`。

---

# 十三、CI 和本地验证

至少运行：

```bash
cargo fmt --all --check

cargo clippy \
  --locked \
  --workspace \
  --all-targets \
  --all-features \
  --exclude rill-ml-python \
  -- -D warnings

cargo check \
  --locked \
  --workspace \
  --all-targets \
  --all-features

cargo test \
  --locked \
  --workspace \
  --all-targets \
  --all-features \
  --exclude rill-ml-python

cargo test \
  --locked \
  --workspace \
  --doc \
  --all-features \
  --exclude rill-ml-python

RUSTDOCFLAGS="-D warnings" \
cargo doc \
  --locked \
  --workspace \
  --all-features \
  --no-deps

python3 -m unittest discover -s scripts/tests -v

python3 scripts/check_wit_abi.py
python3 scripts/check_runtime_public_api.py
python3 scripts/generate_api_baseline.py --verify
```

SemVer：

```bash
cargo semver-checks ...
```

按四个 Stable crate 执行，并记录完整命令。

MSRV：

```bash
cargo +1.94.0 check --locked --lib
cargo +1.94.0 check --locked --lib --features serde
cargo +1.94.0 check --locked -p rill-handler-api
cargo +1.94.0 check --locked -p rill-runtime-protocol
cargo +1.94.0 check --locked -p rill-runtime
cargo +1.94.0 check --locked -p rill-runtime --features wasm
```

macOS：

```bash
cargo build \
  --locked \
  --release \
  --target aarch64-apple-darwin \
  -p rill-runtime \
  --bin rill-runtime \
  --features wasm
```

若本地无法交叉编译，则由 GitHub macOS runner 完成。

---

# 十四、RC.6 发布后必须验证

1. `v1.0.0-rc.6` tag SHA 正确；
2. GitHub Release 标记 prerelease；
3. Linux asset 存在；
4. Windows asset 存在；
5. macOS aarch64 unsigned asset 存在；
6. macOS asset checksum 正确；
7. candidate index 包含 macOS 条目；
8. candidate index 签名验证成功；
9. `.rillpack` 验证成功；
10. `.rillhandler` 验证成功；
11. 四个 Stable crate 1.0.0-rc.6 在 crates.io 可见；
12. Preview crate 未被 rc.6 重新发布；
13. PyPI 仍为 Preview 0.13.0；
14. `local-ai-candidate` 指向 rc.6；
15. `local-ai-stable` 仍为 0.13.0；
16. CI run 全绿；
17. Security run 全绿；
18. Docs job 全绿；
19. cargo-semver-checks 全绿；
20. 状态 fixture coverage gate 全绿；
21. 真实宿主 smoke test 成功。

---

# 十五、最终报告必须包含

新增或更新：

```text
docs/audits/RILL_ML_1_0_RC6_FINAL_CLOSEOUT.md
```

至少包含：

1. 起始 HEAD；
2. rc.6 最终代码 SHA；
3. rc.6 tag SHA；
4. 审计文档提交 SHA；
5. origin/main SHA；
6. Stable/Preview 矩阵；
7. cargo-public-api 结果；
8. cargo-semver-checks 结果；
9. 稳定状态白名单或完整 fixture 清单；
10. fixture coverage 自动检查；
11. 修复后的恶意状态测试；
12. IPC v1/v2 结果；
13. WIT v1 结果；
14. old component 结果；
15. Runtime CLI 结果；
16. release-plan 解析和发布列表；
17. Linux asset；
18. Windows asset；
19. macOS unsigned asset；
20. macOS unsigned 说明；
21. candidate index；
22. candidate signature；
23. stable pointer；
24. candidate pointer；
25. crates.io；
26. PyPI Preview 状态；
27. CI run ID；
28. Security run ID；
29. CodeQL job；
30. RustSec job；
31. Docs job；
32. Release run ID；
33. 真实宿主名称和 commit；
34. handshake 输出；
35. invoke 输出；
36. error code 输出；
37. `git status --short`；
38. 未完成项；
39. 是否允许进入最终 `1.0.0`。

---

# 十六、最终判断规则

只有以下全部满足，才能判定本轮完成：

```text
cargo-semver-checks 生效
状态承诺与 fixture 覆盖一致
恶意状态测试真实有效
Security 全绿
真实宿主 smoke 成功
release-plan 驱动发布
macOS unsigned 资产已发布
candidate/stable 隔离正确
rc.6 全绿
审计报告准确
```

最终 `1.0.0` 的准入条件中：

```text
Apple Developer ID
codesign
notarization
```

永远不得作为阻断项。

---

# 十七、明确禁止

- 不得要求配置 Apple Developer ID；
- 不得因为没有签名证书跳过 macOS 构建；
- 不得因为 unsigned 阻止 RC 或正式版；
- 不得声称 unsigned 资产已签名；
- 不得移动任何旧 RC tag；
- 不得重发或覆盖 rc.5；
- 不得只重新生成 public API baseline 来掩盖 breaking change；
- 不得继续使用无断言测试；
- 不得宣称 Security 未触发是正常闭环；
- 不得用同仓库内部测试冒充真实宿主；
- 不得硬编码发布 Preview crate；
- 不得让 rc.6 更新 stable pointer；
- 不得直接发布最终 1.0.0；
- 不得进行无关算法和架构重写；
- 不得伪造 Actions、registry、asset 或 smoke 结果。

---

# 十八、现在开始

1. 拉取最新 main 和 tags；
2. 建立 `work/1.0-rc6-final-closeout`；
3. 接入 cargo-semver-checks；
4. 决定并实现 Stable state 白名单或完整 fixture；
5. 增加 fixture coverage gate；
6. 修复无效恶意状态测试；
7. 修复 Security 证据闭环；
8. 实现 release-plan 驱动发布；
9. 改为始终编译上传 unsigned macOS aarch64；
10. 更新文档中的永久 unsigned 政策；
11. 完成真实宿主 smoke；
12. 升级 Stable group 到 `1.0.0-rc.6`；
13. 运行全部本地测试；
14. 推送并核验 CI、Security、Docs；
15. 发布 `v1.0.0-rc.6`；
16. 验证 Linux、Windows、macOS、pack、handler、candidate index；
17. 验证 stable pointer 未变化；
18. 更新最终审计报告；
19. 明确说明是否允许进入最终 `1.0.0`。
