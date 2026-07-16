# Rill Handler ABI RFC

> 状态：已冻结（Frozen）
>
> 基线：RillML 0.7.0
>
> 替代：RUNTIME_HANDLER_PLUGIN_PLAN.md Phase 0 spike 结果

## 1. 决策摘要

| 决策项 | 选择 | 理由 |
|---|---|---|
| ABI 技术 | WebAssembly Component Model + WIT | 跨平台、沙箱化、语言无关 |
| Host 运行时 | `wasmtime = "46"` (46.0.0+) | 兼容 Rust 1.94 MSRV（wasmtime 46.x MSRV = 1.94）；46.x 是最新 stable 版本线 |
| Host 绑定生成 | `wasmtime::component::bindgen!` 宏 | 内置于 wasmtime，无需额外依赖 |
| Guest 绑定生成 | `wit-bindgen` (handler 作者自选版本) | rill-handler-api 只提供 WIT，不锁定 guest 工具链 |
| Guest 编译目标 | `wasm32-unknown-unknown` + `wasm-tools component new` | 无 WASI 依赖，组件最精简 |
| MSRV 策略 | 核心库 1.94；`wasm` feature 1.94 | wasmtime 46.x MSRV = 1.94 ≤ 1.94 |

## 2. WIT 世界定义

```wit
package rill:handler@1.0.0;

record handler-metadata {
    id: string,
    version: string,
    api-version: u32,
    capabilities: list<string>,
}

variant handler-error {
    invalid-model(string),
    invalid-input(string),
    unsupported-capability(string),
    execution-failed(string),
}

world invoke-handler {
    export metadata: func() -> handler-metadata;
    export configure: func(model-json: list<u8>) -> result<_, handler-error>;
    export invoke: func(
        capability: string,
        input-json: list<u8>,
    ) -> result<list<u8>, handler-error>;
}
```

### 2.1 ABI 约束

- `HANDLER_API_VERSION` 从 `1` 开始，独立于 host IPC API 和 model pack format。
- `metadata()` 必须与签名 handler manifest 完全一致，否则加载失败。
- `configure()` 每个 runtime 进程只调用一次，接收已验证 model pack 中的 canonical JSON。
- `invoke()` 输入输出是 UTF-8 JSON 字节，不把 `serde_json::Value` ABI 暴露给 guest。
- runtime 负责 JSON 解析、大小限制和 capability 交集；handler 负责业务 schema 校验。
- 首版允许实例内状态，但所有状态只存活于当前 runtime 进程。
- 首版串行执行 invoke；`WasmInvokeHandler` 用互斥保护 store/instance。

## 3. Handler 包格式（.rillhandler）

```text
example.rillhandler
├── manifest.json
├── handler.wasm
├── checksums.json
└── META-INF/signature.ed25519
```

### 3.1 Manifest

```json
{
  "formatVersion": 1,
  "id": "org.example.handler",
  "version": "1.0.0",
  "handlerApiVersion": 1,
  "minRuntimeVersion": "0.7.0",
  "publisherKeyId": "example-handler-key",
  "capabilities": ["org.example.predict"],
  "moduleSha256": "64-lowercase-hex",
  "moduleSize": 123456
}
```

`moduleSha256` 和 `moduleSize` 由 `rill-pack create-handler` 从实际 WASM 字节自动计算并注入，源 manifest 模板中可省略这两个字段。

### 3.2 校验要求

- 固定文件白名单：`manifest.json`、`handler.wasm`、`checksums.json`、`META-INF/signature.ed25519`。
- 拒绝额外文件、重复条目和路径穿越；目录条目（如 `META-INF/`）静默跳过，不视为文件。
- 限制单文件 4 MiB、压缩后总大小 8 MiB、解压后总大小 16 MiB、压缩比 100:1。
- manifest 使用 `deny_unknown_fields`，字符串、数组数量和长度均有上限。
- 版本字段必须是合法 semver。
- `moduleSha256`、checksums 和实际字节必须一致。
- 签名覆盖 canonical manifest 与 checksums，不依赖 ZIP 元数据。
- 签名完成后才允许编译或实例化 WASM。
- handler trust store 与 model trust store 必须由调用方分别提供，不能自动合并。

## 4. Capability 规则

```text
effective_capabilities = model_manifest.capabilities ∩ handler_manifest.capabilities
```

- 两边能力集合都必须非空且无重复值。
- 默认要求模型声明的每个 capability 都被 handler 覆盖；首版不接受静默丢失能力。
- handler 的额外 capability 不会出现在握手中，也不能被调用。
- `RuntimeEngine` 必须在调用 handler 前完成 effective capability 检查。
- guest `metadata()` 返回的 capability 必须与签名 handler manifest 一致。

## 5. WASM 沙箱限制

| 资源 | 默认上限 | 说明 |
|---|---|---|
| Module 字节大小 | 4 MiB | handler.wasm 原始大小 |
| 线性内存 | 64 MiB | 每个实例 |
| Table 大小 | 10_000 entries | 每个实例 |
| WASM 栈大小 | 1 MiB | 防止栈耗尽 trap |
| configure fuel | 10_000_000 units | 独立预算 |
| invoke fuel | 1_000_000 units | 每次调用独立 |
| epoch deadline | 5 秒 | wall-clock 等价 |
| 输入 JSON | 1 MiB | 与 IPC MAX_MESSAGE_BYTES 一致 |
| 输出 JSON | 1 MiB | 与 IPC MAX_MESSAGE_BYTES 一致 |

### 5.1 权限模型

- 禁止 filesystem、network、environment、stdio 和 process 权限。
- 只在确有算法需求时提供受控 random/monotonic-clock imports。
- 不允许 guest 日志写入 stdout，避免破坏 runtime JSON IPC。
- handler trap、超时或非法输出后，runtime 必须仍能返回 health/error 响应。

## 6. 版本策略

| 常量 | 0.7.0 值 | 说明 |
|---|---|---|
| `RUNTIME_API_VERSION` | 2 | 握手增加 handler 身份 |
| `MODEL_PACK_FORMAT_VERSION` | 1 | model 包内容未变 |
| `HANDLER_PACKAGE_FORMAT_VERSION` | 1 | 新增 |
| `HANDLER_API_VERSION` | 1 | 新增 |
| `RELEASE_INDEX_SCHEMA_VERSION` | 2 | 增加 Handler artifact kind |

### 6.1 IPC v1/v2 共存策略

0.7.0 runtime 同时接受 v1 和 v2 请求。v1 握手响应不包含 handler 身份字段；v2 握手响应包含 `handlerId`、`handlerVersion`、`handlerApiVersion` 和 `effectiveCapabilities`。两个 wire schema 分别保存 fixture。

内置 handler（如 `--builtin-handler linear-regression`）不走 WIT ABI，握手响应中 `handlerApiVersion` 报告为 `0`，表示"不适用"。WASM handler 报告 `HANDLER_API_VERSION`（当前为 1）。

### 6.2 错误码

| 错误码 | 含义 |
|---|---|
| `handlerLoadFailed` | handler 包加载、签名或版本校验失败（启动时失败，进程退出；不作为 IPC 响应返回） |
| `handlerTrap` | WASM 执行 trap |
| `handlerTimeout` | fuel 或 epoch 耗尽 |
| `handlerOutputTooLarge` | 输出超过上限 |
| `handlerInvalidOutput` | 输出非 UTF-8 或非法 JSON |
| `handlerCapabilityMismatch` | guest metadata 与签名 manifest 不一致（加载阶段触发，映射为 `handlerLoadFailed`） |
| `handlerInternalError` | 其他宿主侧错误（含 handler 通过 WIT result 返回的错误），不泄漏内部细节 |

## 7. 信任域分离

- model 和 handler 使用独立签名密钥和独立 trust store。
- `--trust-key` 作为 model key 的兼容别名，不得同时授权 handler 代码。
- `--handler-trust-key` 单独授权 handler 代码。
- release index 中 Handler artifact 使用与 model 相同的 release index 签名，但 handler 包自身签名使用 handler 专用密钥。
