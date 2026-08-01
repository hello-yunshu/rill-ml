# Rill Runtime 独立产品与发布契约

RillML 仍然是可嵌入的 Rust 算法库；`rill-runtime` 是建立在它之上的独立本地推理产品。宿主应用只需要编译很小的 `rill-runtime-protocol`，不需要把 RillML 引擎链接进自身，因此 Runtime 和模型包都能脱离宿主应用单独更新。

## 产物

| 产物 | 职责 | 更新单位 |
|---|---|---|
| `rill-ml` | 在线学习算法与状态原语 | Rust crate |
| `rill-handler-api` | 版本化 WIT handler ABI 契约 | Rust crate |
| `rill-runtime-protocol` | 稳定、严格、带版本的 JSON IPC 类型 | Rust crate |
| `rill-runtime` | 加载签名模型包与签名 handler，在沙箱内执行推理 | 独立可执行文件 |
| `*.rillpack` | 模型定义、参数、校验和与 Ed25519 签名 | 独立模型包 |
| `*.rillhandler` | WASM handler 模块、manifest、校验和与 Ed25519 签名 | 独立 handler 包 |
| `stable-index.json` | Runtime/模型/handler 的版本、平台、URL、大小与 SHA-256 | Ed25519 签名发布索引 |

Runtime 使用 stdin/stdout 上的逐行 JSON。每条消息最大 1 MiB，未知字段、未知 API 版本、过长请求 ID 和不合法数值都会被拒绝。握手会返回 Runtime、模型包、handler 身份和能力版本，宿主必须先验证握手，再接受推理结果。

## Handler 插件架构

`rill-runtime` 在启动时加载经过签名和校验的 WASM handler。Handler 通过 WebAssembly Component Model 实现具体 capability；更新 handler 不需要重新编译或替换 `rill-runtime` 二进制。

有效能力集合：

```text
effective_capabilities = model_manifest.capabilities ∩ handler_manifest.capabilities
```

- 模型声明的每个 capability 都必须被 handler 覆盖，否则启动失败。
- Handler 的额外 capability 不会出现在握手中，也不能被调用。
- Guest `metadata()` 返回的 capability 必须与签名 handler manifest 一致。

Handler 在 Wasmtime 沙箱内执行：无 WASI 权限（不访问文件系统、网络、环境变量、stdio 和进程），每次调用有独立 fuel 预算和 epoch 超时，内存上限 64 MiB，table 上限 10 000 条目，I/O JSON 上限 1 MiB。

## CLI

```bash
# 使用签名 WASM handler
rill-runtime serve \
  --pack model.rillpack \
  --handler example.rillhandler \
  --model-trust-key model-key=PUBLIC_KEY_HEX \
  --handler-trust-key handler-key=PUBLIC_KEY_HEX

# 使用内置线性回归 handler（兼容路径，已弃用）
rill-runtime serve \
  --pack model.rillpack \
  --builtin-handler linear-regression \
  --model-trust-key model-key=PUBLIC_KEY_HEX
```

`--handler` 与 `--builtin-handler` 互斥。不指定 handler 时默认使用内置线性回归并打印弃用提示。

## IPC 协议版本

- API v1（0.5.0 起）：握手返回 runtime 与 model 身份，不包含 handler 字段。
- API v2（0.7.0 起）：握手增加 `handlerId`、`handlerVersion`、`handlerApiVersion` 和 `effectiveCapabilities`。
- API v3（Preview）：独立类型支持 observe、decide、feedback、inspect、snapshot、reset，以及显式 deadline、feature schema hash、model/state generation 和 retryability。

Runtime 根据请求的 `apiVersion` 选择响应格式。V1 响应完全省略 handler 字段；V2 响应包含完整 handler 身份。两个 wire schema 是独立的类型，不使用带大量 `Option` 字段的结构冒充两个版本。

V3 当前是 opt-in 的开发接口，不改变 `RUNTIME_API_VERSION = 2`，也不修改
v1/v2 fixture。`StatefulRuntimeEngineV3::handle_at` 由调用方传入时钟，先校验
消息大小、capability、deadline、特征 schema 与 generation，再调用 Preview
Stateful Handler v2。任何 trap、timeout、非法或超大输出/next state 都
fail-closed，不会提交 Runtime 持有的状态。

## 安全与回退

- `.rillpack` 和 `.rillhandler` 只允许固定文件集合，拒绝路径穿越、重复文件、额外载荷、缺失校验和和未知发布密钥。
- Handler 与 model 使用独立的 trust store，不能自动合并。模型密钥不能签署 handler，反之亦然。
- 发布索引的签名覆盖每个二进制、模型包和 handler 包的 SHA-256、大小、版本、平台与 URL。
- Runtime 更新、模型更新与 handler 更新彼此独立，但都必须通过启动自检后才能切换为 `current`。
- 宿主应用使用同文件系统目录重命名完成 `staging → current`，保留一个 `rollback`；激活失败会自动恢复。
- macOS Runtime 除发布索引签名外还必须通过 `codesign --verify --strict`。
- Runtime 缺失、超时、崩溃、包损坏、API 不兼容、模型数据不足或候选误差没有胜过基线时，宿主继续使用确定性回退。
- Handler trap、超时或非法输出后，runtime 进程仍能返回 health/error 响应。

模型包保存经过签名的不可变参数；在线学习状态和具体业务语义由宿主应用管理。

## 本地开发

```bash
cargo build -p rill-runtime --bins
cargo test -p rill-runtime-protocol -p rill-runtime
cargo test -p rill-runtime --features wasm
```

创建模型包时，私钥种子只从环境变量读取：

```bash
export RILL_SIGNING_KEY_HEX='<32-byte Ed25519 seed as hex>'
cargo run -p rill-runtime --bin rill-pack -- create \
  --manifest models/example-default/manifest.json \
  --model models/example-default/model.json \
  --output example.rillpack
```

创建 handler 包：

```bash
export RILL_SIGNING_KEY_HEX='<32-byte Ed25519 seed as hex>'
cargo run -p rill-runtime --bin rill-pack -- create-handler \
  --manifest handler-manifest.json \
  --module handler.wasm \
  --output example.rillhandler
```

检查 handler 包：

```bash
cargo run -p rill-runtime --bin rill-pack -- inspect-handler \
  --handler example.rillhandler \
  --key-id handler-key \
  --public-key-hex PUBLIC_KEY_HEX
```

宿主应用可自行实现调试覆盖逻辑（如通过环境变量指向本地构建的 `rill-runtime` 二进制和 `.rillpack` 模型包）；`rill-runtime` 本身不读取这些环境变量，所有路径和密钥均通过 CLI 参数显式传入。

## 正式发布

`.github/workflows/pipeline.yml` 在 `workflow_dispatch`（由 `Auto Release` 在 `vX.Y.Z` 标签上触发）时执行完整的发布流程：

1. `cargo package --dry-run` 验证 crate 可发布；
2. 为 Linux、Windows 和 Apple Silicon macOS 编译 `rill-runtime` 二进制；
3. 签名模型包与稳定索引；
4. 创建单个 GitHub Release，包含所有平台二进制、`.rillpack`、`.rillhandler` 和 `stable-index.json`。
5. 将 `rill-handler-api`、`rill-runtime-protocol`、`rill-ml`、`rill-runtime` 等发布到 crates.io。

工作流允许在部分发布失败后安全重跑：已经发布的 crate 会跳过，已经存在的版本 Release 会复用其不可变资产，并继续修复 `local-ai-stable` 索引指针。已发布的版本标签不得移动或覆盖。

发布前必须配置：

- `RILL_SIGNING_KEY_HEX`：必须对应发布者内置的 Ed25519 公钥；RillML 示例使用 `rillml-examples-2026-001` 密钥对，生产部署应使用独立密钥对；
- `CARGO_REGISTRY_TOKEN`：crates.io 发布令牌；
- `APPLE_CERTIFICATE_P12_BASE64`、`APPLE_CERTIFICATE_PASSWORD`、`APPLE_SIGNING_IDENTITY`：macOS Developer ID 代码签名（未配置时 macOS 二进制照常构建，仅跳过 codesign）；
- 模型 manifest、workspace 版本与标签版本必须完全一致。

Mira 集成使用独立于示例发布和插件发布的密钥：

- GitHub Secret：`MIRA_SIGNING_KEY_HEX`；
- Key ID：`mira-rill-2026-001`；
- Ed25519 公钥：`3d5ada931abc747610850f9e405fee637a3f445c0b4406eb96b41673814fc261`。

该 Secret 供 Mira 的模型包与 `local-ai-bundle` 发布链使用；通用示例 release 仍使用
`RILL_SIGNING_KEY_HEX`，避免两个信任域互相覆盖。

仓库不会保存或生成生产私钥，也不会把未签名的示例文件冒充正式发布。

## 工具链版本来源

RillML 与 Rill Runtime 的可复现构建依赖以下固定来源的工具链版本。任何升级都应通过 PR 显式 bump 并重跑 CI 验证。

### Rust 工具链

| 工具 | 来源 | 版本约束 | 说明 |
|---|---|---|---|
| `rustc` / `cargo` | `dtolnay/rust-toolchain` Action | stable（CI 默认） | 主构建工具链，跟随 Rust 官方 stable 通道 |
| MSRV 校验 | `dtolnay/rust-toolchain` Action | `1.94.0` 固定 | 见 [pipeline.yml](.github/workflows/pipeline.yml) 的 `msrv` job |
| Rust 缓存 | `Swatinem/rust-cache@v2` | major 固定 | 仅影响构建速度，不影响产物 |

### WASM 工具链

| 工具 | 来源 | 版本约束 | 说明 |
|---|---|---|---|
| `wasm-pack` | `cargo install --locked --version "^0.13"` | 0.13.x | 见 [pipeline.yml](.github/workflows/pipeline.yml) 的 `wasm` job；不使用 `curl \| sh` 安装 |
| `wasm-tools` | `cargo install --locked --version "^1.0"` | 1.x | 见 [pipeline.yml](.github/workflows/pipeline.yml) 的 `wasm-handler` job；用于 WASM 组件包装 |
| `wasm32-unknown-unknown` target | `dtolnay/rust-toolchain` Action | 跟随 stable | 由 `targets: wasm32-unknown-unknown` 字段安装 |

### Python 工具链

| 工具 | 来源 | 版本约束 | 说明 |
|---|---|---|---|
| `maturin` | [scripts/requirements-dev.txt](scripts/requirements-dev.txt) | `>=1.7,<2.0` | Python 绑定构建工具 |
| `pytest` | [scripts/requirements-dev.txt](scripts/requirements-dev.txt) | `>=8,<9` | Python 测试运行器 |
| `pip` | 系统 Python | 跟随 runner | 通过 `pip install -r scripts/requirements-dev.txt` 安装 |

### GitHub Actions

| Action | 固定方式 | 升级策略 |
|---|---|---|
| `actions/checkout` | major（`@v7`） | Dependabot 自动 PR |
| `actions/upload-artifact` | major | Dependabot 自动 PR |
| `actions/download-artifact` | major | Dependabot 自动 PR |
| `dtolnay/rust-toolchain` | `stable` / `master` 分支 | 跟随 Rust 工具链 |
| `Swatinem/rust-cache` | major（`@v2`） | Dependabot 自动 PR |
| `github/codeql-action` | major | Dependabot 自动 PR |

### 版本一致性校验

发布前 CI 通过 [scripts/sync_version.py](scripts/sync_version.py) 校验以下位置的版本必须一致：

- `Cargo.toml` 的 `[workspace.package].version`（唯一真实来源）；
- 模型 manifest、handler manifest、Python `pyproject.toml`；
- GitHub Release 标签 `vX.Y.Z`；
- README 不再硬编码具体版本号，改用 `cargo add rill-ml` 形式。

### 升级流程

升级任何工具链版本时：

1. 在 PR 中显式修改约束；
2. 重跑完整 CI（`pipeline.yml`）；
3. 若涉及 MSRV 变更，更新 `Cargo.toml` 的 `rust-version` 字段并在 CHANGELOG 说明；
4. 若涉及 WASM/Python 工具，确认 [scripts/requirements-dev.txt](scripts/requirements-dev.txt) 与 workflow 中的 `cargo install` 版本同步。
