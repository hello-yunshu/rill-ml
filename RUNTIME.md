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

Runtime 根据请求的 `apiVersion` 选择响应格式。V1 响应完全省略 handler 字段；V2 响应包含完整 handler 身份。两个 wire schema 是独立的类型，不使用带大量 `Option` 字段的结构冒充两个版本。

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

仓库不会保存或生成生产私钥，也不会把未签名的示例文件冒充正式发布。
