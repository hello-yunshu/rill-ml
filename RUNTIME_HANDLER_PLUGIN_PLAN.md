# Rill Runtime 可插拔 Handler 改造计划

> 状态：设计与实施计划
>
> 基线：RillML 0.6.0
>
> 建议目标版本：0.7.0

## 1. 目标

将 `rill-runtime` 改造成业务无关的通用运行时。运行时在启动时加载经过签名和校验的
WASM handler，由 handler 实现具体 capability；更新 handler 不再要求重新编译或替换
`rill-runtime` 二进制。

目标产物关系：

```text
rill-runtime
├── 加载并验证 *.rillpack
├── 加载并验证 *.rillhandler
├── 将签名 manifest 中声明的 capability 交给 handler
├── 在受限 WASM 环境内执行 invoke
└── 继续通过稳定、严格的 JSON IPC 服务宿主
```

本计划只定义 Rill 自身的通用协议、包格式、加载器、安全边界、测试与发布能力。任何具体
宿主的业务类型、业务 capability、更新界面和回退策略均不进入 Rill 的实现。

## 2. 当前状态与差距

### 2.1 已具备的基础

RillML 0.6.0 已经具备以下可复用基础：

| 当前能力 | 代码位置 | 可复用结论 |
|---|---|---|
| 通用 `Invoke { capability, input }` IPC | `crates/rill-runtime-protocol/src/lib.rs` | 业务输入输出已经是通用 JSON |
| `InvokeHandler` trait | `crates/rill-runtime/src/server.rs` | 可作为 WASM adapter 的进程内接口 |
| manifest capability 守卫 | `crates/rill-runtime/src/server.rs` | 可在进入插件前拒绝未声明能力 |
| 签名 `.rillpack` | `crates/rill-runtime/src/package.rs` | 可复用固定文件集、哈希、签名和 trust store 设计 |
| 严格逐行 JSON IPC | `crates/rill-runtime/src/bin/rill-runtime.rs` | 已有消息大小、API 版本和字段约束 |
| 真实进程边界测试 | `crates/rill-runtime/tests/runtime_process.rs` | 可扩展为真实 WASM handler 测试 |
| 多平台 runtime 发布 | `.github/workflows/pipeline.yml` | 已覆盖 Linux、Windows、Intel/Apple Silicon macOS |

### 2.2 当前尚未插件化的部分

- 发布的 `rill-runtime` 在 `serve` 启动时固定构造
  `LinearRegressionInvokeHandler`。
- `InvokeHandler` 是 Rust trait，只能用于同一编译产物内的依赖注入，不能作为稳定动态
  库 ABI。
- 当前没有 handler manifest、handler 包格式、handler 签名或 handler trust domain。
- `ReleaseArtifactKind` 只有 `Runtime` 和 `Model`。
- 握手只报告 runtime 与 model 身份，不能证明实际加载了哪个 handler。
- 当前没有 WASM 执行时的内存、fuel、epoch deadline、输出和宿主权限约束。
- `rill-ml-wasm` 面向浏览器和 `wasm-bindgen`，不是原生 `rill-runtime` 可加载的插件
  ABI，二者必须保持独立。

### 2.3 改造结论

现有 `RuntimeEngine` 和 IPC 不需要推倒重写。合理的改造方式是在现有
`InvokeHandler` 前增加一个稳定的 WASM 边界：

```text
RuntimeEngine
    -> Arc<dyn InvokeHandler>
        -> WasmInvokeHandler
            -> versioned WIT component
```

## 3. 设计原则

1. **Rill 保持业务无关。** ABI 只认识 handler 身份、版本、capability 和 JSON 字节。
2. **不跨边界暴露 Rust ABI。** 不把 trait object、Rust struct layout、panic 或 allocator
   约定暴露给插件。
3. **代码和数据分开信任。** handler 与 model 使用独立签名包和独立 trust store。
4. **先验证后实例化。** 包形状、签名、版本和 capability 必须在执行 WASM 前完成校验。
5. **默认最小权限。** 不开放文件系统、网络、环境变量、进程或任意宿主函数。
6. **所有资源有界。** 模块、内存、执行 fuel、执行时间、输入和输出均有硬上限。
7. **启动时加载，更新后重启。** 首版不做进程内热替换，避免跨调用状态和回滚复杂度。
8. **错误必须可诊断。** 包错误、兼容错误、trap 和资源耗尽使用不同的稳定错误码。
9. **保留确定性回退路径。** 内置 handler 可以作为兼容路径存在，但不得依赖隐式模型
   类型猜测。

## 4. 明确不做

- 0.7.0 不支持 dylib、`.so`、`.dylib` 或 `.dll` handler。
- 不定义 Rust trait 的跨动态库 ABI。
- 不提供运行中无重启热替换。
- 不把 handler 包塞进 `.rillpack`；代码包和模型包保持独立。
- 不在 Rill 中实现具体宿主的下载器、UI、自动更新策略或业务回退算法。
- 不允许 handler 直接访问宿主进程内存或设备接口。
- 不将浏览器用 `rill-ml-wasm` 改造成多用途 ABI 包。

## 5. 目标架构

### 5.1 Crate 与模块布局

建议新增一个面向 handler 作者的小型 crate，并在现有 runtime 内增加 host 实现：

```text
crates/
├── rill-handler-api/
│   ├── Cargo.toml
│   ├── wit/rill-handler.wit
│   └── src/lib.rs
├── rill-runtime-protocol/
│   └── src/lib.rs
└── rill-runtime/
    └── src/
        ├── handler/
        │   ├── mod.rs
        │   ├── builtin.rs
        │   └── wasm.rs
        ├── package.rs
        ├── handler_package.rs
        └── server.rs
```

职责边界：

- `rill-handler-api`：版本化 WIT、guest 侧最小辅助 API 和示例。
- `rill-runtime-protocol`：host IPC、handler manifest、release artifact 和身份类型。
- `rill-runtime::handler_package`：handler 包构建、校验、检查和签名。
- `rill-runtime::handler::wasm`：WASM host、资源限制和 `InvokeHandler` adapter。
- `rill-runtime::server`：只负责 IPC 语义和 capability 守卫，不感知业务 handler。

不建议在首版再拆出独立的 WASM host crate。先通过 `rill-runtime` feature 隔离较重依赖，
待真实体积和复用需求明确后再决定是否拆分。

### 5.2 Handler ABI

首选 WebAssembly Component Model + WIT。设计阶段必须先做 MSRV、跨平台和产物体积
spike；只有该 spike 不能满足发布约束时，才回退到手写 core-WASM ABI。

建议的 WIT 世界：

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

ABI 约束：

- `HANDLER_API_VERSION` 从 `1` 开始，独立于 host IPC API 和 model pack format。
- `metadata()` 必须与签名 handler manifest 完全一致，否则加载失败。
- `configure()` 每个 runtime 进程只调用一次，接收已验证 model pack 中的 canonical JSON。
- `invoke()` 输入输出是 UTF-8 JSON 字节，不把 `serde_json::Value` ABI 暴露给 guest。
- runtime 负责 JSON 解析、大小限制和 capability 交集；handler 负责业务 schema 校验。
- 首版允许实例内状态，但所有状态只存活于当前 runtime 进程。
- 首版串行执行 invoke；`WasmInvokeHandler` 可以用互斥保护 store/instance，不能虚假承诺
  并行安全。

### 5.3 Handler 包格式

新增 `.rillhandler` 固定格式：

```text
example.rillhandler
├── manifest.json
├── handler.wasm
├── checksums.json
└── META-INF/signature.ed25519
```

建议 manifest：

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

包校验要求：

- 固定文件白名单，拒绝目录、额外文件、重复条目和路径穿越。
- 限制单文件、压缩后、解压后总大小以及压缩比。
- manifest 使用 `deny_unknown_fields`，字符串、数组数量和长度均有上限。
- 版本字段必须是合法 semver。
- `moduleSha256`、checksums 和实际字节必须一致。
- 签名覆盖 canonical manifest 与 checksums，不依赖 ZIP 元数据。
- 签名完成后才允许编译或实例化 WASM。
- handler trust store 与 model trust store 必须由调用方分别提供，不能自动合并。

实现时应抽取当前 `.rillpack` 的通用 archive/signature 校验骨架，避免复制一套逐渐漂移的
ZIP 安全逻辑；model 和 handler 仍保留各自独立的 manifest 与错误类型。

### 5.4 Capability 规则

有效能力集合定义为：

```text
effective_capabilities = model_manifest.capabilities ∩ handler_manifest.capabilities
```

启动约束：

- 两边能力集合都必须非空且无重复值。
- 默认要求模型声明的每个 capability 都被 handler 覆盖；首版不接受静默丢失能力。
- handler 的额外 capability 不会出现在握手中，也不能被调用。
- `RuntimeEngine` 必须在调用 handler 前完成 effective capability 检查。
- guest `metadata()` 返回的 capability 必须与签名 handler manifest 一致。

### 5.5 WASM 沙箱

建议使用满足 Rust 1.94 MSRV 的 Wasmtime 版本，并固定 major 版本。启用功能应最小化，
禁止使用默认 feature 集合直接进入生产依赖。

首版必须具备：

- module/component 字节大小上限；
- 线性内存和 table 上限；
- 每次 `configure`/`invoke` 独立 fuel 预算；
- epoch interruption 或等价 wall-clock deadline；
- host 输入输出大小上限；
- trap 到稳定错误的映射；
- 禁止 filesystem、network、environment、stdio 和 process 权限；
- 只在确有算法需求时提供受控 random/monotonic-clock imports；
- 不允许 guest 日志写入 stdout，避免破坏 runtime JSON IPC；
- handler trap、超时或非法输出后，runtime 必须仍能返回 health/error 响应。

具体默认数值不要在实现前拍脑袋确定。Phase 0 spike 应记录官方示例、恶意 fixture 和发布
二进制的实际峰值，再将上限固化为具名常量与测试。

### 5.6 CLI

目标 CLI：

```bash
rill-runtime serve \
  --pack model.rillpack \
  --handler example.rillhandler \
  --model-trust-key model-key=PUBLIC_KEY_HEX \
  --handler-trust-key handler-key=PUBLIC_KEY_HEX
```

辅助命令：

```bash
rill-pack create-handler \
  --manifest handler-manifest.json \
  --module handler.wasm \
  --output example.rillhandler

rill-pack inspect-handler \
  --handler example.rillhandler \
  --trust-key handler-key=PUBLIC_KEY_HEX
```

迁移规则：

- 显式 `--handler` 选择 WASM handler。
- 内置线性回归 handler 迁移到 `--builtin-handler linear-regression`。
- 0.7.0 可以暂时保留旧的 `serve --pack` 自动选择行为，但必须打印弃用提示，并在文档中
  写明后续移除版本。
- `--handler` 与 `--builtin-handler` 互斥，不能按 model `kind` 隐式选择插件。
- 原 `--trust-key` 可以作为 model key 的兼容别名，但不得同时授权 handler 代码。

### 5.7 Host IPC 与版本

当前 IPC API、model pack format 和 release index schema 都是版本 `1`。插件身份进入握手会
改变严格响应结构，因此不能作为 0.6.x 的透明字段追加。

建议：

- 将 `RUNTIME_API_VERSION` 升为 `2`。
- 保持 `MODEL_PACK_FORMAT_VERSION = 1`，除非 model 包内容本身发生变化。
- 将 `RELEASE_INDEX_SCHEMA_VERSION` 升为 `2`。
- 新增 `HANDLER_PACKAGE_FORMAT_VERSION = 1` 和 `HANDLER_API_VERSION = 1`。
- release index artifact 增加 `Handler`，并把 kind-specific 约束集中在
  `ReleaseArtifact::validate_shape()`。
- 0.7 runtime 不应被标记为兼容只支持 API v1 的宿主；发布索引必须阻止错误升级。

握手至少增加：

```text
handler_id
handler_version
handler_api_version
effective_capabilities
```

如果最终决定同时服务 IPC v1 和 v2，必须将两个 wire schema 分模块冻结并分别保存 fixture，
不能依赖一个带大量 `Option` 字段的结构同时冒充两个协议版本。

### 5.8 Release Index

`ReleaseArtifactKind` 增加 `Handler`：

| kind | 平台字段 | 必需版本字段 | 说明 |
|---|---|---|---|
| Runtime | 必须有 OS/arch | runtime API | 平台相关可执行文件 |
| Model | 必须为空 | model format/runtime API | 平台无关数据包 |
| Handler | 必须为空 | handler API/min runtime | 平台无关 WASM 包 |

签名 release index 必须在下载前提供足够的兼容信息。handler 工件身份去重至少包含
`kind + id`；如果未来允许同一 handler API 的不同 profile，再显式增加 profile 字段，不能
复用 OS/arch 表达非平台语义。

Rill 只负责定义、签名和验证这些产物及兼容信息。选择更新时机、下载目录、`current` 指针、
回滚保留数量和 UI 均属于宿主职责。

## 6. 分阶段实施

### Phase 0：技术 spike 与决策冻结

目标：在改公共协议前消除 Component Model、MSRV、体积和资源控制的不确定性。

任务：

- 选择满足 Rust 1.94 的 Wasmtime/wit-bindgen 版本组合。
- 构建一个不依赖业务类型的 echo component。
- 在 Linux、Windows、macOS 上完成 host 加载和一次 invoke。
- 验证 fuel、epoch deadline、memory limiter 和无 WASI 权限配置。
- 记录 debug/release 编译时间、runtime 二进制体积、冷启动和单次 invoke 基线。
- 验证 guest crate 能使用 Rill 核心的确定性算法。
- 验证需要随机数的算法所需最小 imports；不得因此开放完整 WASI 权限。
- 在 `HANDLER-RFC.md` 冻结 ABI、包格式、错误码和版本策略。

退出条件：

- 三个平台 spike 全部通过。
- 资源限制能被自动化测试触发。
- MSRV 检查可运行。
- Component Model 若被否决，RFC 必须记录可复现原因和替代 ABI，而不是只凭体积感觉决定。

### Phase 1：协议与签名包

目标：先实现可独立验证的 handler 产物，不接入执行器。

任务：

- 新增 `rill-handler-api` 和 WIT fixture。
- 在 `rill-runtime-protocol` 增加 handler manifest、identity、artifact kind 和版本常量。
- 抽取 model/handler 共用的安全 archive 骨架。
- 实现 `build_signed_handler_pack()`、`load_handler_pack()` 和 inspection 类型。
- 为 `rill-pack` 增加 `create-handler` 与 `inspect-handler`。
- 增加 canonical JSON、篡改、重复条目、额外文件、压缩炸弹、未知 key 和 trust-domain
  分离测试。

退出条件：

- handler 包可以构建、检查并做签名 round-trip。
- 任一字节篡改都能稳定失败。
- 尚未执行任何未验证的 WASM 字节。
- `cargo package` 覆盖新增 crate。

### Phase 2：WASM InvokeHandler

目标：将签名 handler 安全适配到现有 `RuntimeEngine`。

任务：

- 实现 `WasmInvokeHandler`。
- 在实例化前验证 manifest、签名、版本和 capability。
- 实现 `metadata`、`configure` 和 `invoke` 调用。
- 加入 memory/fuel/deadline/input/output 限制。
- 定义稳定错误码和可安全展示的错误消息；不得泄漏宿主路径或内部 backtrace。
- 保证 handler trap 后 runtime 进程仍可继续处理请求，或明确进入可诊断 unhealthy 状态。
- 保留并显式封装 built-in handler。

退出条件：

- echo、有效推理、非法 JSON、trap、无限循环、内存耗尽和 capability mismatch fixture 全部
  通过。
- handler 无法访问未授权宿主资源。
- `RuntimeEngine` 仍不包含具体业务类型。

### Phase 3：CLI 与 IPC v2

目标：让官方 `rill-runtime` 二进制成为真正可加载插件的通用产品。

任务：

- 增加 `--handler`、`--handler-trust-key` 和显式 built-in 选择。
- 握手报告 handler identity 和 effective capabilities。
- 将 handler 加载失败映射为启动错误，将 invoke 失败映射为响应错误。
- 保存 IPC v1/v2 JSON fixture；按 RFC 决定是否双版本服务。
- 扩展真实进程测试，覆盖签名 pack + 签名 handler + handshake + invoke。
- 确保 stderr 有界且 stdout 只包含协议 JSON。

退出条件：

- 不重新编译 runtime 即可替换 handler 包并在重启后观察到新 handler 版本。
- 错误签名或不兼容 handler 无法启动服务。
- 握手身份与实际执行实例一致。
- 老协议宿主不会误把 API v2 runtime 当作兼容更新。

### Phase 4：发布链与文档

目标：让插件能力具备可重复、可审计的正式发布路径。

任务：

- CI 增加 guest component 构建和跨平台 host 测试。
- MSRV job 覆盖 `rill-handler-api`、`rill-runtime-protocol` 和带 WASM feature 的
  `rill-runtime`；如果依赖无法满足 1.94，必须先做显式 MSRV 决策。
- release index helper 支持 Handler artifact，并增加保留已有 runtime/model/handler 的
  更新测试。
- release workflow 发布 `rill-handler-api` crate 和一个业务无关的官方 example handler。
- release asset verification 覆盖 `.rillhandler`、哈希、大小、版本和签名索引。
- 更新 `RUNTIME.md`、`README.md`、`README.en.md`、`CHANGELOG.md`、`SECURITY.md` 和
  `THIRD_PARTY_NOTICES.md`。
- 记录 Wasmtime/wit-bindgen 的依赖、许可证、升级策略和安全公告响应方式。

退出条件：

- 正式 release dry-run、跨平台 CI、签名资产验证全部通过。
- 发布文档不再要求消费方为自定义 handler 构建自己的 runtime。
- Rill 官方示例能只替换 `.rillhandler`，复用同一份 runtime 二进制完成端到端验收。

## 7. 预计文件改动

| 文件/目录 | 计划改动 |
|---|---|
| `Cargo.toml` | 加入 `rill-handler-api`、WASM host 依赖和 feature |
| `Cargo.lock` | 锁定经过 MSRV 与许可证验证的依赖 |
| `crates/rill-handler-api/` | 新 WIT ABI、guest SDK、示例与 fixture |
| `crates/rill-runtime-protocol/src/lib.rs` | handler 类型、API v2、release schema v2 |
| `crates/rill-runtime/src/package.rs` | 抽取共用安全包校验基础 |
| `crates/rill-runtime/src/handler_package.rs` | `.rillhandler` 构建、加载、签名与检查 |
| `crates/rill-runtime/src/handler/` | built-in 与 WASM adapters |
| `crates/rill-runtime/src/server.rs` | effective capability 与 handler identity |
| `crates/rill-runtime/src/bin/rill-runtime.rs` | 通用 handler CLI |
| `crates/rill-runtime/src/bin/rill-pack.rs` | handler 打包/检查命令 |
| `crates/rill-runtime/tests/` | 包安全、沙箱和真实进程集成测试 |
| `scripts/` | release index 和资产验证支持 Handler |
| `.github/workflows/pipeline.yml` | guest、沙箱、MSRV、跨平台测试与 handler SDK/example handler 发布 |
| `RUNTIME.md` | 正式运行与消费契约 |
| `SECURITY.md` | 不可信 handler 威胁模型与响应策略 |

## 8. 测试矩阵

### 8.1 正常路径

- handler package build/load/inspect round-trip；
- model + handler capability 完全匹配；
- configure 成功后多次 invoke；
- 重启后加载相同 handler；
- 替换 handler 包后 runtime 二进制哈希保持不变；
- Linux、Windows、macOS 真实子进程 handshake + invoke。

### 8.2 包和签名攻击

- manifest、module、checksums、signature 任一字节篡改；
- 未知 publisher key；
- 用 model key 签署 handler；
- 重复 ZIP entry、目录、绝对路径、`..`、额外文件；
- 超大文件、解压后超限、异常压缩比；
- manifest/module metadata 不一致；
- 非法 semver、未知 format/API version、重复 capability。

### 8.3 WASM 攻击与故障

- 无限循环；
- 内存持续增长；
- 递归/栈耗尽；
- configure/invoke trap；
- 超大输出；
- 非 UTF-8 或非法 JSON 输出；
- 未声明 import；
- 文件、网络、环境变量和 stdio 访问尝试；
- capability 欺骗；
- guest metadata 与签名 manifest 不一致。

### 8.4 兼容性

- runtime 太旧；
- handler API 太新；
- model runtime API 不兼容；
- IPC v1/v2 行为符合 RFC；
- release index v1/v2 不被错误混用；
- built-in handler 迁移与弃用提示；
- handler 更新不要求替换平台 runtime。

## 9. 发布门槛

以下条件全部满足后，才可以宣称 Rill Runtime 支持可插拔 handler：

- 存在公开、版本化且有 fixture 的 handler ABI。
- handler 包在执行前完成固定形状、哈希、签名和版本校验。
- model 与 handler trust domain 分离。
- runtime 对 WASM 内存、执行量、时间、输入和输出均有硬限制。
- capability 由 model、handler 和 guest metadata 三方一致性保护。
- 官方二进制可加载外部 handler，不含示例以外的业务逻辑。
- handler trap 或恶意输入不会绕过 IPC 限制或获得宿主权限。
- 真实跨进程和跨平台测试通过。
- crate package、MSRV、许可证、安全审计和 release asset 验证通过。
- 文档明确描述兼容、更新、失败和弃用行为。

## 10. PR 拆分顺序

建议按以下顺序提交，避免一个 PR 同时改变协议、执行器和发布链：

1. `docs: freeze runtime handler RFC`
2. `feat(handler-api): add versioned WIT guest contract`
3. `feat(protocol): add handler manifests and release identities`
4. `feat(pack): add signed rillhandler packages`
5. `feat(runtime): add sandboxed WASM InvokeHandler`
6. `feat(runtime): add generic handler CLI and IPC v2 identity`
7. `test(runtime): add malicious handler and process fixtures`
8. `ci: publish and verify handler artifacts`
9. `docs: finalize runtime plugin operations and security guidance`

每个 PR 都必须保持 workspace 测试通过。协议、包格式或 WIT 一旦进入正式 release，就只能
通过显式版本升级演进，不能直接修改已经发布的 wire fixture。
