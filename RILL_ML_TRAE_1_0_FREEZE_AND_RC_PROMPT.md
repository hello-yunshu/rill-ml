# RillML 1.0 冻结与 `1.0.0-rc.1` 准备提示词

你将继续处理 GitHub 仓库：

- 仓库：`https://github.com/hello-yunshu/rill-ml`
- 默认分支：`main`
- 上一次审计确认的基线 HEAD：`dea7b0c766729ed6f8ad04277814999b5436d87c`
- 当前已发布稳定版本：`0.13.0`
- 当前目标：完成一次真正的 **1.0 API、状态格式、协议与产品契约冻结**
- 本阶段最终目标：在所有冻结条件满足后发布 **`1.0.0-rc.1`**
- 本阶段禁止直接发布最终 `1.0.0`

开始前必须重新拉取远端最新代码和标签。不得假设上述 HEAD 仍是最新状态。

---

# 一、总目标

本轮不是继续扩展算法，也不是再次做泛化代码审计。

本轮只解决：

> 哪些 Rust API、Python/WASM API、状态 JSON、IPC、WIT、模型包、handler 包、CLI 和发布行为将在 1.x 周期内保持兼容。

完成后应达到：

1. 1.0 稳定范围明确；
2. 不应冻结的内部接口已在 RC 前清理；
3. 稳定 Rust API 有自动 SemVer 闸门；
4. 0.13.0 产生的稳定状态能够被 RC 加载，或有明确迁移路径；
5. 所有公开 `from_json` 恢复路径执行状态校验和输入大小限制；
6. IPC v1/v2 有完整 golden fixtures；
7. WIT v1 ABI 有旧组件兼容测试；
8. Runtime CLI 和默认 feature 行为完成最终定稿；
9. 发布系统支持 prerelease；
10. RC 不得更新正式 stable 指针；
11. 文档和安全支持政策与 1.0 承诺一致；
12. 完整 CI、Security、Docs、RC Release 全绿。

---

# 二、执行原则

## 2.1 先拉取和确认状态

首先执行：

```bash
git status --short
git branch --show-current
git fetch --prune --tags origin
git rev-parse HEAD
git rev-parse origin/main
git rev-parse v0.13.0
git log --oneline --decorate -20
git rev-list --left-right --count origin/main...HEAD
```

要求：

- 工作区存在用户未提交修改时，不得覆盖；
- 不得 `reset --hard`；
- 不得 force push；
- 不得移动、删除或覆盖 `v0.13.0`；
- 不得重新发布 `0.13.0`；
- 不得把历史 release 资产替换成新内容；
- 先建立独立工作分支，例如：

```text
work/1.0-freeze
```

## 2.2 不重复已完成工作

不得重新改动已经完成并通过审计的内容，除非 1.0 冻结直接要求：

- FTRL serde 不变量；
- StandardScaler 恶意状态校验；
- ticker 生命周期与 CI；
- WASM fuel / epoch / memory / table / I/O 限制；
- handler 错误脱敏；
- 签名模型包和 handler 包；
- stable index 签名；
- 0.13.0 发布一致性；
- 数学算法实现和已有测试。

## 2.3 子 Agent

可以使用最多两个子 Agent 并行调查，但不能让它们直接同时修改重叠文件。

建议：

### Agent A：API 与状态冻结

负责：

- Rust 公共 API 清单；
- config、enum、模块路径；
- serde 状态格式；
- Python/WASM 恢复校验；
- SemVer baseline。

### Agent B：协议、Runtime 与发布冻结

负责：

- IPC fixtures；
- WIT ABI；
- model/handler package；
- Runtime CLI；
- prerelease channel；
- SECURITY / STABILITY / release policy。

主 Agent：

- 决定最终稳定范围；
- 合并方案；
- 修改代码；
- 运行全量验证；
- 发布 RC；
- 输出最终证据。

---

# 三、第一阶段：确定 1.0 稳定范围

必须先生成一份稳定性矩阵，不能直接把整个 workspace 机械升级到 1.0。

新增：

```text
STABILITY.md
```

至少列出：

| 产物 | 1.0 状态 | 稳定承诺 |
|---|---|---|
| `rill-ml` | Stable / Preview | Rust API、serde 状态 |
| `rill-runtime-protocol` | Stable / Preview | IPC v1/v2 JSON |
| `rill-handler-api` | Stable / Preview | WIT ABI v1 |
| `rill-runtime` | Stable / Preview | Rust API、CLI、包加载 |
| `rill-ml-python` | Stable / Preview | Python 类、方法、异常、JSON |
| `rill-ml-wasm` | Stable / Preview | JS/WASM 类、方法、JSON |
| `rill-ml-tokio` | Stable / Preview | Rust adapter API |
| `rill-ml-arrow` | Stable / Preview | Rust adapter API |
| `rill-ml-polars` | Stable / Preview | Rust adapter API |
| `rillml-inspect` | Stable / Tooling | CLI 输出和参数 |

## 3.1 默认建议

优先采用以下方案：

```text
Stable:
- rill-ml
- rill-runtime-protocol
- rill-handler-api
- rill-runtime

Preview:
- rill-ml-python
- rill-ml-wasm
- rill-ml-tokio
- rill-ml-arrow
- rill-ml-polars
- rillml-inspect
```

但是，如果独立版本会导致发布系统复杂度明显高于把当前小型 API 一次性稳定下来，可以选择统一进入 1.0。

无论选择哪一种，都必须：

- 在 `STABILITY.md` 中说明；
- 在 README 中说明；
- 在 CHANGELOG 中说明；
- 在发布脚本中准确实现；
- 不允许出现“版本号为 1.0，但文档又说 API 随时破坏”的矛盾。

## 3.2 如果 Preview crate 不进入 1.0

必须解除当前“所有 workspace package 必须同版本”的硬约束：

- 允许稳定 crate 使用 `1.0.0-rc.1`；
- Preview crate 保持 `0.x`；
- release workflow 只发布本次 release plan 指定的 crate；
- 内部 path dependency 的版本要求必须正确；
- 不得让 Preview crate 被误发为 `1.0.0-rc.1`；
- 版本同步脚本必须幂等；
- 版本校验脚本必须理解“稳定组”和“Preview 组”。

建议新增机器可读文件：

```text
release-plan.toml
```

示例结构：

```toml
[stable]
version = "1.0.0-rc.1"
crates = [
  "rill-handler-api",
  "rill-runtime-protocol",
  "rill-ml",
  "rill-runtime",
]

[preview]
crates = [
  "rill-ml-python",
  "rill-ml-wasm",
  "rill-ml-tokio",
  "rill-ml-arrow",
  "rill-ml-polars",
  "rillml-inspect",
]
```

不强制使用此结构，但最终必须有单一、可验证的版本来源。

---

# 四、第二阶段：Rust 公共 API 最终缩面

这是 1.0 前最后一次允许进行破坏性公共 API 整理。

## 4.1 生成完整公共 API 清单

对每个 Stable Rust crate 生成公共 API 报告。

推荐使用：

- `cargo-public-api`；
- `cargo-semver-checks`；
- 或等价、可重复的 rustdoc JSON 检查。

不得只使用普通 rustdoc HTML grep。

提交 baseline，例如：

```text
api-baseline/
  rill-ml.txt
  rill-runtime-protocol.txt
  rill-handler-api.txt
  rill-runtime.txt
```

Baseline 必须由 `1.0.0-rc.1` 候选代码生成。

## 4.2 清理双重模块路径

当前存在：

```rust
pub mod linear_regression;
pub use linear_regression::{LinearRegression, LinearRegressionConfig};
```

导致实现模块路径和 re-export 路径同时成为公共合同。

逐个检查：

```text
models
optim
loss
stats
preprocessing
metrics
drift
bandit
diagnostics
runtime handler/package/server 等
```

对不需要下游直接访问的实现模块：

```rust
mod linear_regression;
pub use linear_regression::{...};
```

保留一个官方公共路径。

要求：

- README、rustdoc、examples 全部改用官方路径；
- 编译测试验证旧的非官方路径不再暴露；
- 不要隐藏用户确实需要的模块；
- 不要为了“API 少”而破坏合理的分组入口。

## 4.3 清理内部与测试 API

重点处理 `rill-runtime` 当前公开的：

```text
CapturingLogSink
EngineResponse
```

原则：

- 测试工具不得出现在默认生产公共 API；
- 文档称为 internal 的类型不得直接公开；
- 能设为 `pub(crate)` 就不要 `pub`；
- 测试需要时放入 `#[cfg(test)]`；
- 若下游确实需要，则重新定义为正式稳定 API，并删除“test-only/internal”描述。

同时检查其他 crate 中类似的：

- test helper；
- inspection internal type；
- debug-only accessor；
- fixture helper；
- 不必要的 public field。

## 4.4 可扩展 enum

逐个分类 enum。

### 应考虑 `#[non_exhaustive]`

例如：

- `RillError`
- `Optimizer`
- `RegressionLoss`
- `HandlerLoadError`
- `ModelPackError`
- `HandlerPackError`
- 其他未来可能新增 variant 的非版本化 enum。

### 不应机械添加

版本化 wire schema，例如：

- `RuntimeRequest`
- `RuntimeResponse`
- `RuntimeResponseV2`

它们应通过新增 v3 类型演进，而不是向旧类型追加字段或 variant。

要求在代码和 `STABILITY.md` 中解释分类原则。

## 4.5 Config struct

逐个检查所有公开 config：

- 是否允许下游 struct literal；
- 未来是否可能新增字段；
- 是否已有 `Default`；
- 是否适合 builder 或构造器；
- 是否应加 `#[non_exhaustive]`。

对于预计继续扩展的 config：

- 加 `#[non_exhaustive]`；
- 提供稳定构造方式；
- 确保 examples 不再依赖完整 struct literal，或保留明确稳定的字段集合。

不得把所有 config 机械改成复杂 builder。

## 4.6 Public panic 审计

检查 Stable crate 所有公开 API：

- 禁止因普通用户输入 panic；
- `Mutex::lock().expect(...)` 等不得存在于稳定公开调用路径；
- 内部 invariant panic 必须不可由外部输入触发；
- rustdoc 中准确写 `# Errors` / `# Panics`。

---

# 五、第三阶段：状态格式冻结

## 5.1 不再只依赖当前版本 roundtrip

现有测试主要证明：

```text
当前代码序列化
→ 当前代码反序列化
```

必须新增跨版本 fixtures。

从不可变标签：

```text
v0.13.0
```

生成并保存代表性状态 fixture。

建议使用临时 worktree：

```bash
git worktree add ../rill-ml-v0.13.0 v0.13.0
```

不得修改 tag。

新增：

```text
tests/fixtures/state/v0.13.0/
tests/fixtures/state/v1/
```

至少覆盖所有 Stable serde 类型。

## 5.2 状态版本模型

不得继续只依赖一个无法感知内部结构变化的外层：

```rust
Snapshot<T> {
    format_version,
    model,
}
```

选择以下方案之一。

### 方案 A：稳定 DTO

为核心状态建立明确 DTO：

```text
LinearRegressionStateV1
StandardScalerStateV1
FtrlRegressorStateV1
...
```

内部模型与 DTO 分离。

### 方案 B：类型级 schema version

至少包含：

```json
{
  "formatVersion": 1,
  "modelType": "linearRegression",
  "modelSchemaVersion": 1,
  "model": {}
}
```

### 方案 C：冻结当前 serde schema

只有在逐字段确认并提交 golden fixtures、aliases、defaults 和迁移规则后才允许采用。

最终必须保证：

- 私有 Rust 字段重命名不会无意破坏 1.x 状态；
- 新增可选字段有默认值；
- enum serde 表示明确；
- schema bump 有迁移说明；
- 旧 fixture 在所有未来 1.x CI 中持续加载。

## 5.3 状态验证 trait

为公开可恢复状态建立统一验证接口，例如：

```rust
pub trait ValidateState {
    fn validate_state(&self) -> Result<(), RillError>;
}
```

名称可调整，但必须实现统一语义。

至少覆盖：

- 维度与向量长度；
- 所有有限数值；
- 非负计数和统计量；
- optimizer 参数数；
- FTRL `max_features`；
- encoder 映射一致性；
- pipeline transformer/model 维度；
- bandit arm 状态；
- drift detector buffer；
- 所有可通过 Python/WASM 恢复的类型。

## 5.4 `Snapshot` 恢复

提供稳定的 validated restore：

```rust
snapshot.into_validated_model()
```

或等价接口。

要求：

- 版本检查；
- 类型状态校验；
- 错误原子；
- 不激活半有效状态；
- 文档明确 trusted / untrusted input 区别。

如保留原 `into_model()`：

- 明确它只适用于可信状态；
- Python/WASM 不得调用不校验版本。

## 5.5 输入大小限制

Python 和 WASM 的 `from_json` 必须在反序列化前限制输入字节数。

新增统一常量，例如：

```text
MAX_SNAPSHOT_JSON_BYTES
```

建议上限依据实际状态规模决定，不得随意使用无限输入。

测试：

- 正常 fixture 成功；
- 超大 JSON 拒绝；
- 恶意维度拒绝；
- 非有限值拒绝；
- vector 长度错位拒绝；
- optimizer 状态错位拒绝；
- 失败后不返回模型。

## 5.6 Python/WASM 恢复

所有：

```text
from_json
```

改为 validated restore。

不能继续仅执行：

```rust
snap.into_model()
```

---

# 六、第四阶段：IPC v1/v2 冻结

## 6.1 Golden fixtures

新增：

```text
crates/rill-runtime-protocol/tests/fixtures/
  v1/
  v2/
```

覆盖：

### Request

- handshake；
- health；
- invoke。

### Response v1

- handshake；
- health；
- result；
- error。

### Response v2

- handshake；
- health；
- result；
- error。

每个 fixture 进行：

1. fixture JSON → 当前类型；
2. 当前类型 → JSON；
3. 规范化后与 fixture 一致；
4. unknown field 仍拒绝；
5. 错误 API version 由 runtime 正确拒绝。

## 6.2 稳定错误码

不要继续只用散落的字符串。

建立：

```rust
pub mod error_code {
    pub const INVALID_JSON: &str = "...";
    ...
}
```

或稳定的 `ErrorCode` 类型。

至少冻结：

- `invalidJson`
- unsupported API version；
- invalid request；
- capability error；
- handler timeout；
- handler trap；
- handler output too large；
- handler invalid output；
- handler internal error；
- 其他现有 wire code。

要求：

- v1/v2 fixture 覆盖；
- runtime 只从稳定定义生成；
- 文档列出；
- 后续新增 code 为 additive；
- 现有 code 不得改名。

## 6.3 Protocol 演进规则

写入 `STABILITY.md`：

- v1/v2 类型和 wire schema 永久冻结；
- 新字段或新语义使用 v3；
- 不在旧 schema 中追加 required field；
- Runtime 在整个 1.x 至少支持 v1/v2；
- 删除旧协议只能在 2.0 或明确支持周期后。

---

# 七、第五阶段：WIT ABI v1 冻结

## 7.1 冻结 WIT 源

当前：

```wit
package rill:handler@1.0.0;
```

已经等价于对外宣告 ABI 1.0。

必须新增自动检查：

- 规范化 WIT 文本 hash；
- `HANDLER_API_VERSION`；
- `WIT_VERSION`；
- package version；
- handler manifest API version；

全部一致。

例如新增：

```text
scripts/check_wit_abi.py
```

## 7.2 旧组件兼容 fixture

不能只让 host 和 guest 同时使用当前 WIT 构建。

必须保留一个由冻结 WIT v1 编译好的预构建 component：

```text
crates/rill-runtime/tests/fixtures/handler-v1-component.wasm
```

要求：

- fixture 来源、构建 commit 和 SHA-256 有记录；
- CI 不在每次运行时用当前 WIT 重建此 fixture；
- 当前 runtime 必须成功加载；
- metadata/configure/invoke 全流程通过；
- 打包成旧 `.rillhandler` 后仍可加载；
- manifest API v1 与 WIT v1 一致；
- 修改当前 WIT 而不 bump ABI 时测试失败。

## 7.3 WIT 演进规则

明确：

- additive change 是否允许；
- breaking change 必须新增 `rill:handler@2.x`；
- v1 runtime 支持周期；
- handler API version 与 WIT package major 的映射；
- 新 runtime 在 1.x 期间继续加载 v1 handler。

---

# 八、第六阶段：Runtime 产品契约定稿

## 8.1 默认 WASM feature

当前官方 GitHub 二进制包含 `wasm`，但：

```bash
cargo install rill-runtime
```

默认不包含 WASM。

必须统一行为。

推荐：

```toml
[features]
default = ["wasm"]
wasm = ["dep:wasmtime"]
```

如果担心默认编译体积，则必须：

- 文档明确 `cargo install rill-runtime --features wasm`；
- 默认二进制启动时清晰说明不支持 handler；
- 不得让 README 看起来像默认安装即可加载 `.rillhandler`。

优先让 crates.io 默认安装行为与官方二进制一致。

## 8.2 删除隐式 deprecated 默认 handler

当前未传 handler 时会默认使用已弃用的 built-in linear regression。

在 RC 前完成最终选择。

推荐：

- 未传 `--handler` 或 `--builtin-handler` → 启动失败；
- `--handler` 为正式推荐路径；
- `--builtin-handler linear-regression` 可临时保留为显式兼容路径；
- 不再隐式 fallback；
- 若决定彻底删除 built-in，必须在 RC 前完成。

不得把“已弃用但仍是默认”的矛盾带入 1.0。

## 8.3 CLI 参数

修正：

```text
--model-trust-key
```

为正式主名称。

旧：

```text
--trust-key
```

可以保留为兼容 alias，但：

- 帮助文本只展示正式名称；
- 文档只使用正式名称；
- 测试覆盖两者；
- 明确旧 alias 在 1.x 是否保留。

检查全部 CLI：

- subcommand 名；
- 参数名；
- 必填／默认；
- exit code；
- stdout/stderr；
- inspect JSON 字段。

进入 RC 后不得随意改变。

## 8.4 macOS 签名

正式 RC 和 1.0 不能无声发布 unsigned macOS 产物。

选择：

### 推荐

- Apple signing secrets 缺失 → macOS release job 失败；
- codesign 验证失败 → release 失败。

### 可接受替代

- secrets 缺失时不发布 macOS asset；
- release index 不得声称包含 macOS；
- README 明确该版本未提供官方 macOS。

不得继续：

```text
跳过 codesign
但仍作为官方 macOS 正式产物发布
```

---

# 九、第七阶段：支持 prerelease / candidate 发布

## 9.1 SemVer prerelease

所有版本工具支持：

```text
1.0.0-rc.1
1.0.0-rc.2
```

至少包括：

- `scripts/sync_version.py`
- `scripts/release_version.py`
- Auto Release workflow；
- CI / Release workflow；
- Cargo packages；
- Python metadata；
- model manifest；
- handler manifest；
- CHANGELOG；
- release title。

不要支持不必要的任意格式。

推荐使用标准 SemVer parser，不要靠松散正则。

## 9.2 Candidate channel

新增：

```text
candidate-index.json
```

或同等机制。

Release index channel 至少支持：

```text
stable
candidate
```

要求：

- `1.0.0-rc.1` 只更新 candidate；
- 不更新 `local-ai-stable`；
- 不覆盖 stable index；
- stable pointer 仍指向 `0.13.0`；
- GitHub Release 标记 prerelease；
- crates.io 发布 prerelease；
- PyPI 发布 prerelease；
- candidate index 经过签名；
- candidate pointer 与 stable pointer 使用不同 release/tag。

建议：

```text
local-ai-candidate
local-ai-stable
```

## 9.3 正式版切换

最终 `1.0.0` 才允许：

- 更新 stable index；
- 更新 `local-ai-stable`；
- 标记正式 GitHub Release；
- README 状态改为 Stable。

## 9.4 RC 资产不可变

- `v1.0.0-rc.1` tag 不得移动；
- RC 资产不得覆盖；
- 修复使用 `rc.2`；
- 发布失败可以重跑原 tag，但只能补齐缺失资产；
- 已存在资产必须校验一致；
- 不一致时必须失败。

---

# 十、第八阶段：Python、WASM 和适配器

根据第三阶段稳定性矩阵执行。

## 10.1 如果 Python 进入 1.0 Stable

必须：

- 固定公开类与方法；
- 定义正式异常类型，不要所有错误都只映射为 `PyRuntimeError` 字符串；
- `from_json` validated restore；
- JSON 大小限制；
- Python 3.9–3.13 测试；
- Linux x86_64 wheel；
- Windows x86_64 wheel；
- macOS aarch64 wheel；
- PyPI classifiers 准确；
- 不再声称未构建的平台为“Operating System Independent”；
- 至少有 import/install smoke test；
- wheel matrix 全部由 release workflow 生成。

## 10.2 如果 Python 保持 Preview

必须：

- 版本保持 0.x 或显式 prerelease；
- 文档写明不受核心 1.x 完整 API 稳定承诺；
- 不误标成 `1.0.0` stable；
- release workflow 不把它当作 1.0 阻断产物；
- 仍保持现有安全和测试质量。

## 10.3 WASM

同样选择 Stable 或 Preview。

如果 Stable：

- 固定导出 JS 名称；
- 固定方法名；
- validated restore；
- 输入大小限制；
- JS/TS API fixture；
- Node/browser smoke test；
- npm 发布策略或明确只提供 crate 构建。

## 10.4 Arrow / Polars / Tokio

如果 Stable：

- 生成 API baseline；
- 对上游 major dependency 策略写清楚；
- 不得因上游升级在 1.x 中随意破坏自身 API。

如果 Preview：

- 文档和版本号必须真实反映。

---

# 十一、第九阶段：文档与政策

## 11.1 `STABILITY.md`

至少包括：

- 稳定 crate；
- Preview crate；
- Rust API；
- Python/WASM；
- serde 状态；
- IPC v1/v2；
- WIT v1；
- model/handler package v1；
- CLI；
- MSRV；
- platform support；
- deprecation policy；
- 1.x breaking-change policy。

## 11.2 `SECURITY.md`

更新：

- 1.0 RC 的支持范围；
- 最终 1.x 支持范围；
- runtime、protocol、handler API 是否在 scope；
- Python/WASM 是否在 scope；
- 安全修复回补策略；
- supported versions 表；
- macOS 签名政策；
- Wasmtime 安全升级政策。

## 11.3 `CONTRIBUTING.md`

增加 1.x 规则：

- breaking change 必须进入 2.0；
- 先弃用、后删除；
- deprecation 至少保留多久；
- API baseline 更新审批；
- state schema bump；
- IPC v3 / WIT v2 规则；
- MSRV 提升规则；
- public enum / config 设计规范。

## 11.4 README / ROADMAP / CHANGELOG

修正：

- 不再把 v0.7 写成当前；
- 说明 `0.13.0` 是最后一个 0.x stable；
- `1.0.0-rc.1` 是 candidate；
- stable pointer 仍为 0.13.0；
- 安装命令与 feature 一致；
- 平台支持准确；
- Python/WASM 状态准确；
- RC 不等同于最终 Stable。

## 11.5 修正历史小残留

一并修正此前确认的纯文档残留：

- 审计报告最终 HEAD；
- `wasm.rs` 中仍把 rustdoc grep 称为主要 API 检查的旧注释。

只改文字，不重新开启相关代码工作。

---

# 十二、CI 必须新增的闸门

至少新增以下 jobs / steps。

## 12.1 Public API

```text
public-api-baseline
cargo-semver-checks
```

对 Stable crate 执行。

RC 后：

- additive API 允许；
- breaking API 默认失败；
- baseline 变更必须明确审核。

## 12.2 State compatibility

```text
load v0.13.0 fixtures
load v1 fixtures
malicious state rejection
Python validated restore
WASM validated restore
snapshot size limit
```

## 12.3 Protocol compatibility

```text
IPC v1 golden fixtures
IPC v2 golden fixtures
stable error codes
old client fixture compatibility
```

## 12.4 WIT compatibility

```text
WIT hash/version consistency
load prebuilt WIT v1 component
load prebuilt v1 rillhandler
metadata/configure/invoke
```

## 12.5 Runtime CLI

```text
cargo install/default feature smoke
missing handler fails
explicit builtin compatibility
model-trust-key
trust-key alias
inspect JSON fixture
```

## 12.6 Release channel

```text
stable release plan validation
candidate release plan validation
RC cannot update stable pointer
stable cannot use prerelease version
candidate index signature
immutable RC assets
```

## 12.7 Python wheel matrix

仅在 Python Stable 时加入。

---

# 十三、推荐提交组织

建议按以下提交拆分，不要混成一个巨大提交。

## Commit 1

```text
docs(stability): define the 1.0 compatibility surface
```

## Commit 2

```text
refactor(api): finalize the 1.0 Rust public surface
```

## Commit 3

```text
feat(state): freeze and validate versioned model state
```

## Commit 4

```text
test(protocol): freeze IPC v1/v2 and handler ABI v1
```

## Commit 5

```text
fix(runtime): finalize CLI and handler defaults for 1.0
```

## Commit 6

```text
release(rc): add candidate-channel prerelease publishing
```

## Commit 7

```text
ci(1.0): enforce API, state, protocol and ABI compatibility
```

## Commit 8

```text
release(version): prepare 1.0.0-rc.1
```

## Commit 9

```text
docs(1.0-rc): publish support and compatibility policies
```

若实现范围较小可合并相邻提交，但禁止把无关算法修改混入。

---

# 十四、本地验证

根据实际稳定范围调整，但至少运行：

```bash
cargo fmt --all --check

cargo clippy   --locked   --workspace   --all-targets   --all-features   --exclude rill-ml-python   -- -D warnings

cargo check   --locked   --workspace   --all-targets   --all-features

cargo test   --locked   --workspace   --all-targets   --all-features   --exclude rill-ml-python

cargo test   --locked   --workspace   --doc   --all-features   --exclude rill-ml-python

RUSTDOCFLAGS="-D warnings" cargo doc   --locked   --workspace   --all-features   --no-deps
```

MSRV：

```bash
cargo +1.94.0 check --locked --lib
cargo +1.94.0 check --locked --lib --features serde
cargo +1.94.0 check --locked -p rill-handler-api
cargo +1.94.0 check --locked -p rill-runtime-protocol
cargo +1.94.0 check --locked -p rill-runtime
cargo +1.94.0 check --locked -p rill-runtime --features wasm
```

脚本：

```bash
python3 -m unittest discover -s scripts/tests -v
```

Public API：

```bash
cargo public-api ...
cargo semver-checks ...
```

具体命令根据实际采用工具填写，并把工具版本记录到审计报告。

Python：

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -r scripts/requirements-dev.txt
cd crates/rill-ml-python
maturin develop --release
pytest tests/ -v
```

WASM：

```bash
cargo check --locked -p rill-ml-wasm --target wasm32-unknown-unknown
cd crates/rill-ml-wasm
wasm-pack test --node --release
```

---

# 十五、RC 发布准入条件

只有全部满足，才允许创建 `v1.0.0-rc.1`。

## API

- 稳定范围已写入 `STABILITY.md`；
- 所有 Stable Rust crate 有 API baseline；
- SemVer CI 生效；
- 不应稳定的内部 API 已移除；
- config / enum 策略完成；
- 公共 API 普通输入不 panic。

## State

- v0.13.0 fixtures 已生成并提交；
- RC 能加载所有承诺兼容的旧状态；
- v1 fixtures 已冻结；
- validated restore 生效；
- Python/WASM `from_json` 不接受恶意状态；
- JSON 输入大小有限。

## IPC/WIT

- IPC v1/v2 全 fixtures；
- error code 冻结；
- 旧 WIT v1 component 可加载；
- 旧 `.rillhandler` 可加载；
- WIT/API version 自动一致。

## Runtime

- 默认 WASM 行为与文档一致；
- 不再隐式使用 deprecated handler；
- CLI 参数定稿；
- model trust key 名称正确；
- macOS 官方资产签名策略明确并由 CI 执行。

## Release

- prerelease SemVer 支持；
- GitHub prerelease；
- candidate signed index；
- candidate pointer；
- stable pointer 仍指向 0.13.0；
- RC 不覆盖 stable；
- crates.io/PyPI prerelease 可见；
- tag 与资产不可变。

## Documentation

- README、ROADMAP、CHANGELOG、SECURITY、CONTRIBUTING、STABILITY 一致；
- 没有“0.x experimental”和“1.0 stable”同时存在的矛盾；
- Python/WASM/适配器状态真实；
- 平台支持真实。

---

# 十六、RC 发布后验证

发布 `1.0.0-rc.1` 后必须验证：

1. GitHub Release 标记 prerelease；
2. stable GitHub Release 未变化；
3. `local-ai-stable` 仍指向 `0.13.0`；
4. `local-ai-candidate` 指向 `1.0.0-rc.1`；
5. candidate index 签名成功；
6. runtime Linux/Windows/macOS 资产与 index checksum 一致；
7. `.rillpack` 验证成功；
8. `.rillhandler` 验证成功；
9. crates.io prerelease 可见；
10. PyPI prerelease 可见（若 Python 参与 RC）；
11. 旧 v0.13.0 snapshot 加载成功；
12. 旧 WIT v1 handler 加载成功；
13. IPC v1 client fixture 成功；
14. IPC v2 client fixture 成功；
15. Mira 或至少一个真实宿主完成集成 smoke test。

发现阻断问题时：

- 不移动 `rc.1`；
- 修复后发布 `rc.2`；
- 不直接覆盖；
- 不提前更新 stable。

---

# 十七、最终审计报告

新增：

```text
docs/audits/RILL_ML_1_0_FREEZE_AUDIT.md
```

报告必须包含：

1. 起始 HEAD；
2. 最终代码 HEAD；
3. 最终文档 HEAD；
4. origin/main HEAD；
5. 稳定性矩阵；
6. 每个 Stable crate 的 API baseline；
7. 被移除的非正式公共 API；
8. `#[non_exhaustive]` 决策；
9. config 策略；
10. v0.13.0 state fixture 列表；
11. v1 state fixture 列表；
12. validated restore 覆盖；
13. 恶意状态测试；
14. IPC v1 fixture 列表；
15. IPC v2 fixture 列表；
16. 稳定 error codes；
17. WIT v1 hash；
18. 旧 component SHA-256；
19. 旧 handler 加载测试；
20. Runtime 默认 feature；
21. CLI 最终参数；
22. built-in handler 最终决策；
23. macOS 签名结果；
24. prerelease 版本验证；
25. candidate index；
26. stable pointer 未变化证据；
27. CI run ID；
28. Security run ID；
29. Docs run ID；
30. RC Release run ID；
31. RC tag SHA；
32. GitHub prerelease URL；
33. crates.io 状态；
34. PyPI 状态；
35. candidate index signature；
36. stable/candidate pointer；
37. `git status --short`；
38. 最终提交列表；
39. 未完成项；
40. 是否允许进入 `1.0.0-rc.1`；
41. 是否允许进入最终 `1.0.0`。

不得只写：

```text
已完成
全部通过
```

每项必须有可核验的命令、路径、SHA、run ID 或产物。

---

# 十八、明确禁止

- 不得直接发布最终 `1.0.0`；
- 不得移动 `v0.13.0`；
- 不得覆盖任何已发布资产；
- 不得让 RC 更新 stable pointer；
- 不得用当前代码同时重建 host 和 guest 来冒充 WIT 兼容测试；
- 不得只做 serde 当前版本 roundtrip；
- 不得只写文档声称 API 稳定而没有 CI baseline；
- 不得把所有 public enum 机械加 `non_exhaustive`；
- 不得把版本化 IPC v1/v2 类型随意扩展；
- 不得保留“deprecated 但默认启用”的 Runtime 行为；
- 不得发布 unsigned macOS 资产却称为正式签名产物；
- 不得把 Preview crate 无意标记成 1.0 Stable；
- 不得为了 1.0 新增无关算法；
- 不得做无关架构重写；
- 不得伪造测试、发布和兼容结果。

---

# 十九、现在开始

1. 拉取最新 main 和 tags；
2. 建立 `work/1.0-freeze`；
3. 生成稳定性矩阵；
4. 清点 Stable crate 公共 API；
5. 完成最后一次 API 缩面；
6. 建立 API baseline；
7. 建立 v0.13.0 state fixtures；
8. 实现稳定 state schema 和 validated restore；
9. 冻结 IPC v1/v2；
10. 冻结 WIT v1；
11. 加载旧 WIT component；
12. 定稿 Runtime feature、handler 和 CLI；
13. 实现 candidate 发布通道；
14. 更新支持政策；
15. 运行全部本地测试；
16. 推送并核验 CI / Security / Docs；
17. 准备 `1.0.0-rc.1`；
18. 发布 GitHub prerelease、candidate index 和对应 registry prerelease；
19. 验证 stable pointer 未变化；
20. 完成真实宿主 smoke test；
21. 输出完整审计报告；
22. 明确说明是否已达到最终 `1.0.0` 准入条件。
