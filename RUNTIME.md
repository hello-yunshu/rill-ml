# Rill Runtime 独立产品与发布契约

RillML 仍然是可嵌入的 Rust 算法库；`rill-runtime` 是建立在它之上的独立本地推理产品。宿主应用只需要编译很小的 `rill-runtime-protocol`，不需要把 RillML 引擎链接进自身，因此 Runtime 和模型包都能脱离宿主应用单独更新。

## 产物

| 产物 | 职责 | 更新单位 |
|---|---|---|
| `rill-ml` | 在线学习算法与状态原语 | Rust crate |
| `rill-runtime-protocol` | 稳定、严格、带版本的 JSON IPC 类型 | Rust crate |
| `rill-runtime` | 加载签名模型包并执行本地预测 | 独立可执行文件 |
| `*.rillpack` | 模型定义、参数、校验和与 Ed25519 签名 | 独立模型包 |
| `stable-index.json` | Runtime/模型的版本、平台、URL、大小与 SHA-256 | Ed25519 签名发布索引 |

Runtime 使用 stdin/stdout 上的逐行 JSON。每条消息最大 1 MiB，未知字段、未知 API 版本、过长请求 ID 和不合法数值都会被拒绝。握手会返回 Runtime、模型包和能力版本，宿主必须先验证握手，再接受预测结果。

## 安全与回退

- `.rillpack` 只允许固定文件集合，拒绝路径穿越、重复文件、额外载荷、缺失校验和和未知发布密钥。
- 发布索引的签名覆盖每个二进制和模型包的 SHA-256、大小、版本、平台与 URL；更新器不信任裸网络响应。
- Runtime 更新与模型更新彼此独立，但都必须通过启动自检后才能切换为 `current`。
- Mira 使用同文件系统目录重命名完成 `staging → current`，保留一个 `rollback`；激活失败会自动恢复。
- macOS Runtime 除发布索引签名外还必须通过 `codesign --verify --strict`。
- Runtime 缺失、超时、崩溃、包损坏、API 不兼容、模型数据不足或候选误差没有胜过基线时，宿主继续使用确定性预测。

电量模型当前采用无副作用的历史重放：每次从 Mira 已保存且经插件策略许可的历史中重建在线模型。用户学习数据不会放进 `.rillpack`，更新模型包不会覆盖历史；也不会为了学习增加 HID 读取。以后如果改为增量常驻状态，必须使用独立的版本化状态信封，并保留历史重放恢复路径。

## 本地开发

```bash
cargo build -p rill-runtime --bins
cargo test -p rill-runtime-protocol -p rill-runtime
```

创建模型包时，私钥种子只从环境变量读取：

```bash
export RILL_SIGNING_KEY_HEX='<32-byte Ed25519 seed as hex>'
cargo run -p rill-runtime --bin rill-pack -- create \
  --manifest models/mira-battery-default/manifest.json \
  --model models/mira-battery-default/model.json \
  --output battery.rillpack
```

调试版 Mira 可使用 `RILL_RUNTIME_PATH`、`RILL_MODEL_PACK_PATH` 和 `RILL_TRUST_KEY=key-id=public-key-hex` 指向本地构建；这些覆盖项只在 debug 构建中生效。

## 正式发布

`.github/workflows/runtime-release.yml` 在 `runtime-vX.Y.Z` 标签上构建 Linux、Windows、Intel macOS 和 Apple Silicon macOS Runtime，签名模型包与稳定索引，并发布版本化资产。发布前必须配置：

- `RILL_SIGNING_KEY_HEX`：必须对应 Mira 内置的生产 Ed25519 公钥；
- `APPLE_CERTIFICATE_P12_BASE64`、`APPLE_CERTIFICATE_PASSWORD`、`APPLE_SIGNING_IDENTITY`：macOS Developer ID 代码签名；
- 模型 manifest、workspace 版本与标签版本必须完全一致。

仓库不会保存或生成生产私钥，也不会把未签名的示例文件冒充正式发布。首次可用更新仍需要维护者配置上述 Secrets 并创建真实发布标签。

完整 Runtime release 建立首个稳定通道后，可以只修改模型 manifest/参数并创建 `model-vX.Y.Z` 标签。`.github/workflows/model-release.yml` 会先验证当前稳定索引的签名，保留所有已发布 Runtime 条目，只替换 `mira.battery.default` 模型条目并重新签名。因此模型更新不要求提高 Runtime 或 Mira 的版本。
