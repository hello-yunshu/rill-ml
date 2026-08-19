# RillML 命名规范

本文档是 RillML 项目品牌与命名的一致性地基。它是品牌、简称、技术命名空间与下游生态之间边界的单一权威说明。

## 官方品牌

- **正式品牌名：RillML**（永远是正式名称）。
- **简称：Rill**（display shorthand / conversational shorthand / UI shorthand）。

首次出现时应建立关系：

```text
RillML（简称 Rill）
```

英文：

```text
RillML, referred to as Rill where context is unambiguous
```

之后在无歧义上下文中可使用 `Rill`。正式品牌名（README 标题、Release title、GitHub description、许可证/法律文本、crate 描述、架构文档、稳定性政策）必须使用 **RillML**。

## 简称边界

`Rill` 只是简称，**不代表**项目拥有或试图占用所有裸 `rill` 技术命名空间。禁止：

- 创建裸 `rill` crate / package / module / CLI；
- 把 `rill-ml` 改成 `rill`；
- 把 Python `rill_ml` 改成 `rill`；
- 把 CLI `rill-runtime` 改成 `rill`；
- 增加 `import rill` 兼容 alias。

原因：`Rill` 在 GitHub、crates.io、PyPI、npm、CLI 与现有软件品牌中存在明显重名/占用。

## 命名一览

| 类别 | 名称 |
|---|---|
| 官方品牌 | RillML |
| 简称 | Rill |
| GitHub 主仓库 | `hello-yunshu/rill-ml` |
| 核心 Rust crate | `rill-ml` |
| Rust module | `rill_ml` |
| Runtime CLI | `rill-runtime` |
| 协议 crate | `rill-runtime-protocol` |
| Handler API crate | `rill-handler-api` |
| Handler ABI namespace | `rill:handler` |
| 模型包 | `.rillpack` |
| Handler 包 | `.rillhandler` |
| Python crate | `rill-ml-python`（`import rill_ml`） |
| WASM crate | `rill-ml-wasm` |
| Arrow adapter | `rill-ml-arrow` |
| Polars adapter | `rill-ml-polars` |
| Tokio adapter | `rill-ml-tokio` |
| C FFI crate | `rill-ml-ffi`（`rill_ml.h` / `rill_ml_*`） |
| Inspector CLI | `rillml-inspect` |
| PM adapter | `rill-pm-adapter` |
| 签名身份 | `rillml-examples-2026-001` |

## 正式展示名

技术 crate 保持显式名称，正式展示名（brand display name）统一：

| 技术名 | 正式展示名 | 可简称 |
|---|---|---|
| `rill-runtime` | RillML Runtime | Rill Runtime |
| `rill-runtime-protocol` | RillML Runtime Protocol | — |
| `rill-handler-api` | RillML Handler API | — |
| `.rillpack` | RillML Model Pack | — |
| `.rillhandler` | RillML Handler Pack | — |
| `rill-ml` | RillML adaptive intelligence core library | — |
| `rill-ml-python` | RillML Python bindings | — |
| `rill-ml-ffi` | RillML C FFI | — |

## 品牌文案规范

正式：`RillML`。可简称：`Rill`。

不推荐（除代码 identifier、legacy constant、historical text、language naming constraint 外）：

```text
Rill ML
RILL ML
RillMl
RILLML
```

## UI 规范

UI 空间有限时可以显示 `Rill`（例如状态卡片：`Rill / Mode / Health / Confidence / Drift`）。但 About、Settings、Help、Documentation、first-run 应至少一次显示 `RillML`，从而建立简称关系。

## 下游生态

| 项目 | 正式名称 | 简称 |
|---|---|---|
| `hello-yunshu/rill-xray-agent` | RillML Xray Agent | Rill Xray Agent |
| Performance Manager | RillML（作为 external runtime / advisory-only / bounded / fail-closed） | Rill |
| `contracts/rill-dependency.json` | RillML (Rill) | Rill（component shorthand） |
| `pm-rill-shadow` | 保持 | 保持 |

仓库名与协议名（如 `rill-xray-agent`、`pm-rill-shadow`）不因品牌整理而改名，除非存在极强的技术理由。

## 稳定性

RillML 1.x Stable 合约中的 frozen 名称（`rill-ml` Rust public API、IPC v1/v2、WIT ABI v1、`.rillpack` v1、`.rillhandler` v1、release-index contract）绝不因品牌文案而改名或 bump。
