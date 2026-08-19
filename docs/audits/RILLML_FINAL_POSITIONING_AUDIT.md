# RillML Final Positioning Audit

Audit date: 2026-08-20
Scope: `hello-yunshu/rill-ml` and its direct downstream ecosystem
Final conclusion: **PASS**

## Official brand

- **Official brand: RillML**
- **Short name: Rill** (display shorthand / conversational shorthand / UI shorthand only)
- Relationship established on first mention as `RillML（简称 Rill）` (zh) / `RillML (Rill)` (en).
- Single source of truth: `docs/NAMING.md`.

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

## Downstream classification (2026-08-20 final)

| Repository | HEAD SHA | Classification | Changes | Result |
|---|---|---|---|---|
| `hello-yunshu/rill-ml` | `f85bcd7` | CANONICAL / PASS | NAMING.md / README / audits / GitHub description；本审计按最新 downstream 事实重建 | VERIFIED |
| `hello-yunshu/mira-mouse` | `c6f8a331` | REAL_DEPENDENCY / MODIFIED / VERIFIED | README / README.en.md / CONTRIBUTING.md 首次展示 `RillML（简称 Rill）`；删除过时 `mira-runtime sidecar` 表述；workflow 更名 `Sync Latest RillML Runtime`；docs 引用同步 | PASS |
| `hello-yunshu/mira-mouse-plugins` | — | UNAFFECTED | 无 RillML 引用 | — |
| `hello-yunshu/homebrew-mira` | — | UNAFFECTED | 无 RillML 引用 | — |
| `hello-yunshu/luci-app-performance-manager` | `7d1d60e` | REAL_DEPENDENCY / MODIFIED / VERIFIED | README + README.en.md 首次展示 `RillML (Rill)`；架构图 `RillML upstream runtime`；zh_Hans 翻译门禁已修 | PASS |
| `hello-yunshu/rill-xray-agent` | `3709b02c`（PR #10 已合并 → main） | REAL_INTEGRATION / MODIFIED / VERIFIED | Stable 状态统一 `RillML Xray Agent 0.1.0 (stable)`；PACKAGE_SHA256SUMS 已刷新 | PASS |
| `hello-yunshu/Xray_bash_onekey` | `455a93c9`（PR #83 已合并 → main） | REAL_INTEGRATION / MODIFIED / VERIFIED | README + 5 语言 i18n 首次展示 `RillML（简称 Rill）`；PO/POT 菜单文案为 UI 简称保留 | PASS |
| `hello-yunshu/lessonforge` | `27a87da` | REAL_DEPENDENCY / MODIFIED / VERIFIED | `document-intelligence/pyproject.toml` description → `RillML Assist/MLOps`；README 首次展示 | PASS |
| `hello-yunshu/Xray_bash_onekey_api` | — | TECHNICAL_REFERENCE | `rill-ml` 为安全测试夹具字符串 | UNAFFECTED |
| `hello-yunshu/github-hey-run` | — | TECHNICAL_REFERENCE | `rill-ml` 为 app 注册表技术引用 | UNAFFECTED |
| 其余 owner 仓库 | — | UNAFFECTED | 无 RillML 术语 | — |

跨仓库审计明细见 `RILLML_DOWNSTREAM_NAMING_AUDIT.md`；残留扫描见 `RILLML_DOWNSTREAM_RESIDUAL_SCAN.json`。

## Xray

- `hello-yunshu/rill-xray-agent`：正式名称 **RillML Xray Agent**，简称 **Rill Xray Agent**；README 首屏与 stable 状态均已统一；内部 `RillML*` 命名（`RillMLError` 等）与正式品牌一致，不迁移。
- `hello-yunshu/Xray_bash_onekey`：中文/英文/5 语言 README 首次展示已统一为 `RillML（简称 Rill）` / `RillML (Rill)`；技术名（`rill-xray-agent`、`rill_payload`、`scripts/rill_xray_agent_*`、`systemd/rill-xray-agent-*`、`RillMLError` 族）全部保持。

## Performance Manager

- `hello-yunshu/luci-app-performance-manager`：RillML 作为 external runtime / advisory-only / bounded / fail-closed 保持。
- `contracts/rill-dependency.json` description 已明确 `RillML (Rill)`；`pm-rill-shadow` v1 协议与 `rill-pm-adapter` artifact 名不变。
- LuCI UI h2 已改为 `RillML (Rill) Intelligence`，并补充对应 zh_Hans 翻译以通过 contract-validation 门禁。
- PM 绝不编译 RillML：不新增 Rust 编译依赖，RillML 仅消费官方预编译、签名 artifact。

## Mira

- `hello-yunshu/mira-mouse`：workflow 展示名 `Sync Latest RillML Runtime`；README/README.en.md/CONTRIBUTING.md 首次展示 `RillML（简称 Rill）`；已删除过时的 `mira-runtime sidecar` 架构表述，恢复为"验证 RillML 稳定签名索引后下载 rill-runtime sidecar"。
- 技术 identity 全部保持：`rill-runtime` / `sync-rill-runtime.yml` / `resolve-latest-rill-release.mjs` / `mira-rill-sync` / `mira-rill-2026-002` / `mira-handler-2026-001` 均未改。

## DL roadmap

- 1.x 不实现完整 DL 栈（无 tensor engine / autograd / DL compiler / GPU 内核）。
- 预留 `InferenceProvider` 概念但未提交不成熟 API；不进入 1.x Stable freeze。
- 推荐方向：frozen neural encoder + RillML online adaptive head。

## CI results (Remote GitHub Actions)

### hello-yunshu/rill-ml (main `f85bcd7` — 最终审计提交)

| Workflow | Run ID | Result |
|---|---|---|
| CI / Release | 32300016158 | success |
| Docs | 32300016182 | success |
| Auto Release | 32300686904 | fail-closed（docs-only 变更，拒绝重新发布 v1.2.0，符合预期） |

### hello-yunshu/mira-mouse (main `c6f8a331`)

| Workflow | Run ID | Result |
|---|---|---|
| CI / Release（最终提交 workflow_dispatch） | 32302279764 | success |
| Sync Latest RillML Runtime | 32293787911 | success |

### hello-yunshu/luci-app-performance-manager (main `7d1d60e`)

CI (run 32294828836)：

| Job | Result | 说明 |
|---|---|---|
| static | success | 品牌相关门禁（含 zh_Hans 翻译 contract-validation）通过 |
| pm-rill-provenance | success | |
| openwrt-ucode | success | |
| pm-rill-runtime | failure | pre-existing（truncated-json 已知问题，rc.7 相同失败，非 branding 引入） |
| pm-core-rill-roundtrip | failure | pre-existing（rc.7 相同失败，非 branding 引入） |
| final-release-evidence | skipped | 依赖上述失败 job |

Build OpenWrt (remote SDK)：CI 的 `openwrt-ucode` job 在 run 32294828836 中 success（官方 OpenWrt SDK 编译通过）；run 32294828794 与重跑的 32303995009 均在 `Install host dependencies`（`apt-get update`）阶段挂起——同日 Azure 镜像外部阻塞（与 Xray/mira-mouse 相同），未进入编译。历史 run 32284139817 的 `openwrt-sdk-build` 曾 success（277a243e），证明编译链正常。品牌改动（docs-only）不影响编译。

### hello-yunshu/rill-xray-agent (PR #10 已合并 → main `3709b02c`)

| Workflow | Run ID | Result |
|---|---|---|
| Source Gates + RillML remote consumer (gnu/musl) | 32295317272 / 32295321530 | success |
| PID1 (debian12 / ubuntu2404) | 32295317255 / 32295321531 | success |

### hello-yunshu/Xray_bash_onekey (PR #83 已合并 → main `455a93c9`)

| Workflow | Run ID | Result |
|---|---|---|
| Rill Xray Agent（payload 校验 + no-downstream-build gate + rill 测试） | 32293640162 / 32293643229 | success |
| Test Install（5 模式矩阵） | 32293643225（含重跑） | success — 首轮 tls/ws_grpc_xhttp 因 runner 默认 apt 源 `azure.archive.ubuntu.com` 当日不可达超时（外部硬阻塞，影响所有分支）；重跑后全绿 |
| release-gate | 32293643225（重跑） | success |

## Affected repos

| Repo | 处理 | 状态 |
|---|---|---|
| `hello-yunshu/rill-ml` | README / README.en.md / docs / NAMING.md / STABILITY.md / GitHub description / Cargo.toml / audits | 完成，main 同步（`f85bcd7`） |
| `hello-yunshu/rill-xray-agent` | README 品牌名 + stable 状态 + PACKAGE_SHA256SUMS | 完成，PR #10 已合并（main `3709b02c`） |
| `hello-yunshu/luci-app-performance-manager` | contract description + UI h2 + zh_Hans 翻译 + README.en.md + 审计文档再生成 | 完成，main 同步（`7d1d60e`） |
| `hello-yunshu/Xray_bash_onekey` | README + 5 语言 i18n 首次展示统一 | 完成，PR #83 已合并（main `455a93c9`） |
| `hello-yunshu/mira-mouse` | README / README.en.md / CONTRIBUTING.md / sync workflow 名 / local-ai docs | 完成，main 同步（`c6f8a331`） |
| `hello-yunshu/lessonforge` | pyproject description + README 首次展示 | 完成，master 同步（`27a87da`） |
| `hello-yunshu/mira-mouse-plugins` / `homebrew-mira` | 无修改（无引用） | UNAFFECTED |

## Residual scan

| Check | Result |
|---|---|
| `Rillun` / `rillun` / `RILLUN_` active code | 无（仅 rill-ml 历史审计文档中"无残留"结论引用，标记 HISTORICAL） |
| `.rillunpack` / `.rillunhandler` / `rillun:handler` | 无 |
| `Rill` 被当作官方 package / repo / bare CLI / bare Python module | 无 |
| 裸 `rill` crate / module / CLI 被创建 | 无 |
| 跨仓库品牌术语 | 8 个仓库命中，全部为 OFFICIAL_BRAND / SHORT_NAME / TECHNICAL_IDENTITY；两处技术引用（`Xray_bash_onekey_api` 测试夹具、`github-hey-run` 注册表）无需修改 |

详细命中见 `RILLML_DOWNSTREAM_RESIDUAL_SCAN.json`。

## Conclusion

**PASS**

- 正式品牌 RillML、简称 Rill 的关系已建立并有清晰边界；技术 namespace（`rill-ml` / `rill_ml` / `rill-runtime` / `rill-runtime-protocol` / `rill-handler-api` / `.rillpack` / `.rillhandler` / `rill:handler`）全部保持 1.x frozen。
- 下游分类已按最新事实修正：`Xray_bash_onekey` 与 `mira-mouse` 从 UNAFFECTED 更正为 REAL_INTEGRATION / REAL_DEPENDENCY + MODIFIED / VERIFIED；新增 `lessonforge` 为 REAL_DEPENDENCY。
- Remote Actions：rill-ml 全绿；rill-xray-agent PR #10 已合并且 Source Gates / remote consumer / PID1 全绿；mira-mouse 最终提交 CI（32302279764）与 Sync Latest RillML Runtime 全绿；PM 品牌相关门禁（static / provenance / openwrt-ucode）全部通过（pm-rill-runtime / pm-core-rill-roundtrip 为 rc.7 既有失败，非 branding）；Xray_bash_onekey PR #83 已合并，Test Install 首轮 tls/ws_grpc_xhttp 因 2026-08-19 当日 `azure.archive.ubuntu.com` 不可达超时（外部硬阻塞），重跑后全绿。
- 残余扫描干净，无 Rillun 残留，无裸 `rill` namespace 侵占。
