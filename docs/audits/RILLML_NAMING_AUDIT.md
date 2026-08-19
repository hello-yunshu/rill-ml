# RillML Naming Audit

Audit date: 2026-08-19
Scope: `hello-yunshu/rill-ml` and its direct downstream ecosystem.

## Official brand

- **Official brand: RillML**
- **Short name: Rill** (display shorthand / conversational shorthand / UI shorthand only; NOT a package/module/CLI namespace)

## Repository / package / module / CLI names

| Category | Name | Status |
|---|---|---|
| GitHub repo | `hello-yunshu/rill-ml` | 保持 |
| Core Rust crate | `rill-ml` | 保持 |
| Rust module | `rill_ml` | 保持 |
| Runtime CLI | `rill-runtime` | 保持（正式展示名 RillML Runtime） |
| Protocol crate | `rill-runtime-protocol` | 保持 |
| Handler API crate | `rill-handler-api` | 保持（WIT ABI namespace `rill:handler`） |
| Model pack | `.rillpack` | 保持 |
| Handler pack | `.rillhandler` | 保持 |
| Python crate | `rill-ml-python`（`import rill_ml`） | 保持 |
| WASM / Arrow / Polars / Tokio | `rill-ml-wasm` / `-arrow` / `-polars` / `-tokio` | 保持 |
| C FFI | `rill-ml-ffi`（`rill_ml.h` / `rill_ml_*`） | 保持 |
| Inspector CLI | `rillml-inspect` | 保持（Preview tooling，无稳定外部依赖、迁移成本与发布硬依赖均不满足改名条件） |
| PM adapter | `rill-pm-adapter`（协议 `pm-rill-shadow` v1） | 保持 |
| Signing identity | `rillml-examples-2026-001` | 保持 |

## Downstream names

| Repo | 正式名称 | 简称 | 处理 |
|---|---|---|---|
| `hello-yunshu/rill-xray-agent` | RillML Xray Agent | Rill Xray Agent | README / README_EN 首屏已更新 |
| `hello-yunshu/Xray_bash_onekey` | — | Rill Xray AI（功能名） | UNAFFECTED（内部 `RillML*` 命名与正式品牌一致） |
| `hello-yunshu/luci-app-performance-manager` | RillML | Rill（UI/component shorthand） | contract description + UI h2 已更新 |
| `hello-yunshu/mira-mouse` | RillML | — | UNAFFECTED（已一致使用 RillML） |
| `hello-yunshu/mira-mouse-plugins` | — | — | UNAFFECTED（无引用） |

## Residual ambiguity scan

| Check | Result |
|---|---|
| `Rill ML` / `RILL ML` / `RillMl` / `RILLML` 作为品牌正式名出现在活跃文档 | 无（仅 NAMING.md 不推荐列表与历史/语言约束上下文） |
| `Rill` 被当作官方 package / repo / bare CLI / bare Python module | 无 |
| 裸 `rill` crate / module / CLI 被创建 | 无 |
| Rillun / rillun / RILLUN_ active code | 无 |
| `.rillunpack` / `rillun:handler` | 无 |

## Naming normalization actions

- 新增 `docs/NAMING.md`：官方品牌、简称边界、命名一览、正式展示名、UI 规范、下游生态、稳定性。
- `STABILITY.md`：顶部命名说明（RillML official / Rill contextual shorthand；frozen 名称绝不替换为简称）。
- `README.md` / `README.en.md`：新增 "14. 命名" 章节。
- 品牌文案不推荐形式已在 NAMING.md 列出并避免使用。

## Conclusion

命名一致、无品牌歧义、无裸 `rill` namespace 侵占、无 Rillun 双品牌残留。技术命名空间全部保持 1.x frozen。
