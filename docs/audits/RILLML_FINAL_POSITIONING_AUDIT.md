# RillML Final Positioning Audit

Audit date: 2026-08-19
Scope: `hello-yunshu/rill-ml` and its direct downstream ecosystem
Final conclusion: **PASS**

## Official brand

- **Official brand: RillML**
- **Short name: Rill** (display shorthand / conversational shorthand / UI shorthand only)
- Relationship established on first mention as `RillML (Rill)`.

## Repository / crate / module / CLI / protocol / format

| Category | Name | Status |
|---|---|---|
| GitHub repo | `hello-yunshu/rill-ml` | 保持 |
| Core Rust crate | `rill-ml` | 保持 |
| Rust module | `rill_ml` | 保持 |
| Runtime | `rill-runtime`（正式展示 RillML Runtime） | 保持 |
| Protocol | `rill-runtime-protocol`（IPC v1/v2 frozen） | 保持 |
| Handler API | `rill-handler-api`（WIT ABI `rill:handler` v1） | 保持 |
| Model pack | `.rillpack` v1 | 保持 |
| Handler pack | `.rillhandler` v1 | 保持 |
| Python | `rill-ml-python`（`import rill_ml`） | 保持 |
| C FFI | `rill-ml-ffi`（`rill_ml.h` / `rill_ml_*`） | 保持 |
| Adapter crates | `rill-ml-wasm` / `-arrow` / `-polars` / `-tokio` | 保持 |
| PM adapter | `rill-pm-adapter`（协议 `pm-rill-shadow` v1） | 保持 |
| Inspector CLI | `rillml-inspect` | 保持（Preview tooling，不满足改名条件） |
| Signing identity | `rillml-examples-2026-001` | 保持 |

技术命名空间全部保持 1.x frozen；未制造任何 package / ABI / protocol / format breaking change。

## Xray

- `hello-yunshu/rill-xray-agent`：正式名称 **RillML Xray Agent**，简称 **Rill Xray Agent**；README 首屏已更新；内部 `RillML*` 命名（`RillMLError` 等）与正式品牌一致，不迁移。
- `hello-yunshu/Xray_bash_onekey`：UNAFFECTED（内部命名与正式品牌一致，无 Rillun 残留）。

## Performance Manager

- `hello-yunshu/luci-app-performance-manager`：RillML 作为 external runtime / advisory-only / bounded / fail-closed 保持。
- `contracts/rill-dependency.json` description 已明确 `RillML (Rill)`；`pm-rill-shadow` v1 协议与 `rill-pm-adapter` artifact 名不变。
- LuCI UI h2 已改为 `RillML (Rill) Intelligence`，并补充对应 zh_Hans 翻译以通过 contract-validation 门禁。
- PM 绝不编译 RillML：不新增 Rust 编译依赖，RillML 仅消费官方预编译、签名 artifact。

## DL roadmap

- 1.x 不实现完整 DL 栈（无 tensor engine / autograd / DL compiler / GPU 内核）。
- 预留 `InferenceProvider` 概念但未提交不成熟 API；不进入 1.x Stable freeze。
- 推荐方向：frozen neural encoder + RillML online adaptive head。

## CI results (Remote GitHub Actions)

### hello-yunshu/rill-ml (main `61d7256` — docs(brand) 提交)

| Workflow | Run ID | Result |
|---|---|---|
| CI | 32269427760 | success |
| Docs | 32269427726 | success |
| Security audit | 32269427861 | success |
| Cross-platform verification | 32280502014 | success（重跑；原 32269427868 因 runner 问题取消） |
| Auto Release | 32270102928 | failure — 符合预期的 fail-closed 行为（docs-only 变更，检测到同 SHA 已取消的 Cross-Platform 运行，按 release policy 拒绝发布） |

说明：branding 变更为文档性变更，未触碰稳定 API/ABI/wire/state，Auto Release 拒绝重新发布 v1.2.0 是正确行为。

### hello-yunshu/rill-xray-agent (main `ad598af` — PR #9 已合并)

| Workflow | Run ID | Result |
|---|---|---|
| Source Gates | 32277986079 | success |
| PID1 Systemd | 32275315337 | success |

### hello-yunshu/luci-app-performance-manager (main `277a243`)

CI (run 32284139784)：

| Job | Result | 说明 |
|---|---|---|
| static | success | **已修复**：补充 zh_Hans 翻译后 make audit / contract-validation 通过 |
| pm-rill-provenance | success | |
| openwrt-ucode | success | |
| pm-core-rill-roundtrip | failure | pre-existing（rc.7 相同失败，非 branding 引入） |
| pm-rill-runtime | failure | pre-existing（truncated-json 已知问题，rc.7 相同失败） |
| final-release-evidence | skipped | 依赖上述失败 job |

Build OpenWrt (remote SDK)（run 32284139817）：

| Job | Result | 说明 |
|---|---|---|
| openwrt-sdk-build | success | 官方 OpenWrt SDK 编译通过（branding 改动可正常打包） |
| final-release-evidence | failure | pre-existing release-readiness 门禁（apk-verification.json 缺失；非 release rc 提交同 rc.7 一样失败） |

## Affected repos

| Repo | 处理 | 状态 |
|---|---|---|
| `hello-yunshu/rill-ml` | README / README.en.md / docs / NAMING.md / STABILITY.md / GitHub description / Cargo.toml / audits | 完成，main 同步 |
| `hello-yunshu/rill-xray-agent` | README 品牌名 + PACKAGE_SHA256SUMS | 完成，PR #9 已合并 |
| `hello-yunshu/luci-app-performance-manager` | contract description + UI h2 + zh_Hans 翻译 + 审计文档再生成 | 完成，main 同步 |
| `hello-yunshu/Xray_bash_onekey` | 无修改（内部命名已一致） | UNAFFECTED |
| `hello-yunshu/mira-mouse` / `mira-mouse-plugins` | 无修改（已一致 / 无引用） | UNAFFECTED |

## Residual scan

| Check | Result |
|---|---|
| `Rillun` / `rillun` / `RILLUN_` active code | 无 |
| `.rillunpack` / `rillun:handler` | 无 |
| `Rill Adaptive` / `Rill Loop` active usage | 无 |
| `Rill` 被当作官方 package / repo / bare CLI / bare Python module | 无（Cargo.toml 无裸 `name="rill"`，无 `import rill`） |
| 裸 `rill` crate / module / CLI 被创建 | 无 |
| Rillun 双品牌残留 | 无（仅审计文档中"无"结论引用） |

## Conclusion

**PASS**

- 正式品牌 RillML、简称 Rill 的关系已建立并有清晰边界；技术 namespace（`rill-ml` / `rill_ml` / `rill-runtime` / `rill-runtime-protocol` / `rill-handler-api` / `.rillpack` / `.rillhandler` / `rill:handler`）全部保持 1.x frozen。
- 定位已升级为 "lightweight adaptive intelligence runtime for native and edge applications"，未破坏现有 Stable contract。
- 远程 Actions：rill-ml 与 rill-xray-agent 全绿；PM 的 branding 相关门禁全部通过（含本次修复的 zh_Hans 门禁），仅剩 rc.7 既有的 release-readiness 失败（truncated-json / roundtrip / final-release-evidence），均有据可查且与 branding 无关。
- 残余扫描干净，无 Rillun 残留，无裸 `rill` namespace 侵占。
