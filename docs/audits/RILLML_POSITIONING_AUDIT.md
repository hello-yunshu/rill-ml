# RillML Positioning Audit

Audit date: 2026-08-19
Scope: `hello-yunshu/rill-ml` and its direct downstream ecosystem.

## Current descriptions (before)

| Surface | Old description | Problem |
|---|---|---|
| README.md (first screen) | 面向 Rust 应用、边缘设备和持续数据流的轻量在线机器学习库（隐含） | 将项目局限为"在线 ML 库"，未覆盖 Runtime/签名资产/安全集成 |
| README.en.md (first screen) | Lightweight online machine learning for Rust applications, edge devices, and continuously changing data streams | 同上 |
| GitHub description | Lightweight, serializable online machine learning for Rust applications and streaming data. | 同上 |
| Cargo.toml (rill-ml) | Lightweight, serializable online machine learning for Rust applications and streaming data. | 同上 |
| ROADMAP.md | 面向 Rust 应用、边缘设备和持续数据流的轻量在线机器学习库 | 定位未升级 |
| CONTRIBUTING.md | RillML is an online (single-pass, bounded-memory) machine learning library for Rust | 局限为库 |
| src/lib.rs crate doc | Lightweight, serializable online machine learning for Rust applications and streaming data | 同上 |
| rill-ml-ffi lib.rs / rill_ml.h | the lightweight, serializable online machine learning library | 同上 |

## Problems identified

1. 项目已包含可独立分发的 `rill-runtime`、稳定 IPC v1/v2、签名 `.rillpack` / `.rillhandler` 与安全宿主集成，但首屏定位仍停留在"在线 ML 库"。
2. 定位描述与"产品边界"（online adaptation、bounded resource、signed artifacts、safe execution、state lifecycle）不一致。
3. 英文与中文 README 信息架构不一致，未统一三层架构表达。

## New canonical description

```text
RillML (Rill) is a lightweight adaptive intelligence runtime
for native and edge applications.

It combines online machine learning, bounded adaptation,
stable runtime protocols, signed model/handler assets,
and safe host integration for continuously changing systems.
```

中文：

```text
RillML（简称 Rill）是面向原生应用与边缘设备的轻量自适应智能运行时。

它将在线机器学习、有界自适应、稳定运行时协议、
签名模型/处理器资产以及安全宿主集成组合在一起，
用于持续变化的系统环境。
```

## README changes

- `README.md`：重写为 14 节信息架构（简介 / 为什么选择 / 核心能力 / 架构 / 快速开始 / Runtime / 在线机器学习 / 签名资产 / 安全模型 / 平台支持 / 集成与生态 / 稳定性 / 路线图 / 命名）。
- `README.en.md`：与中文版对齐为同一 14 节架构；首屏改为新定位。
- 首屏同时回答：What is RillML / Why / What is Rill / Who should use it / What it does not try to be。

## Docs changes

- `ROADMAP.md`：项目定位更新为"轻量自适应智能运行时"；长期目标改为"自适应智能运行时"。
- `CONTRIBUTING.md`：定位改为 RillML 运行时 + `rill-ml` 核心库。
- `RUNTIME.md`：标题由 "Rill Runtime 独立产品与发布契约" 改为 "RillML Runtime 独立产品与发布契约"。
- `src/lib.rs`、`crates/rill-ml-ffi/src/lib.rs`、`crates/rill-ml-ffi/include/rill_ml.h`：crate 文档定位同步。
- `STABILITY.md`：文件顶部增加命名说明（RillML official / Rill shorthand），未重写任何冻结条款。
- 新增 `docs/NAMING.md`。

## Architecture consistency

统一三层表达（Online ML / Runtime / Integrations）与宿主视角图：

```text
Host Application → RillML Runtime → { Online ML Core, State/Snapshot, Drift/Decision, Signed Model Packs, Sandboxed Handlers }
```

- `rill-ml` 明确为 "RillML adaptive intelligence core library"，不再是"项目的全部"。
- Runtime / Protocol / Handler 作为独立产品边界保持 frozen 名称。

## DL roadmap boundary

- 当前 1.x 不实现完整 DL。
- 未来方向：frozen neural encoder + RillML online adaptive head；预留 `inference-provider` 抽象（feature-gated / Preview / non-Stable）。
- 明确不加入：full tensor engine / autograd / DL compiler / GPU/NPU kernel framework。

## Crate metadata changes

- 根 `Cargo.toml` (rill-ml) description → "RillML adaptive intelligence core library — lightweight, serializable online machine learning for native and edge applications."
- `rill-runtime-protocol` description → "Versioned IPC and package contracts for the RillML local runtime."
- `rill-handler-api` description → "Versioned WIT ABI and guest SDK for RillML Runtime handlers."
- `echo-handler` description → "Business-neutral echo handler for RillML Runtime — returns input as output."

## Repository metadata changes

- GitHub description → "RillML — lightweight adaptive intelligence runtime for native and edge applications. Online ML, drift detection, bandits, signed runtime assets and safe host integration."

## Downstream positioning

- `rill-xray-agent`：README / README_EN 首屏升级为 "RillML Xray Agent（简称 Rill Xray Agent）"。
- `luci-app-performance-manager`：`contracts/rill-dependency.json` description 首次提及明确 "RillML (Rill)"；LuCI `rill.js` 主视图标题改为 "RillML (Rill) Intelligence"。
- `Xray_bash_onekey`、`mira-mouse`：已一致使用 RillML 正式品牌，无需修改（记录 AFFECTED-ALIGNED / UNAFFECTED）。

## Result

定位从"仅一个在线 ML Rust 库"升级为 "lightweight adaptive intelligence runtime for native and edge applications"，同时保持 RillML 1.x Stable 合约与全部 frozen 技术名称。
