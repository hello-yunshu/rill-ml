# RillML Downstream Naming Audit

Audit date: 2026-08-20
Scope: `hello-yunshu` owner, RillML downstream ecosystem (34 active repositories scanned)
Method: local working-tree scan at HEAD (excludes `.git`, `node_modules`, `target`, `__pycache__`) + `gh` repository/CI enumeration
Final conclusion: **PASS**

## Brand standard

- Official brand: **RillML**
- Short name: **Rill** (display / conversational / UI shorthand only)
- First-mention relationship: `RillML（简称 Rill）` (zh) / `RillML (Rill)` (en)
- Single source of truth: `hello-yunshu/rill-ml/docs/NAMING.md`

## Repository table

| Repository | HEAD SHA | Relationship | Changes | Technical identifiers preserved | CI evidence | Result |
|---|---|---|---|---|---|---|
| `hello-yunshu/rill-ml` | `f85bcd7` | CANONICAL | NAMING.md / README / audits / GitHub description; audit rebuilt to latest facts | `rill-ml` / `rill_ml` / `rill-runtime` / `rill-runtime-protocol` / `rill-handler-api` / `.rillpack` / `.rillhandler` / `rill:handler` — unchanged | CI/Release 32300016158 success; Docs 32300016182 success; Auto Release 32300686904 (fail-closed for docs-only) | VERIFIED |
| `hello-yunshu/mira-mouse` | `c6f8a331` | REAL_DEPENDENCY / MODIFIED | README / README.en.md / CONTRIBUTING.md first-mention `RillML（简称 Rill）`; dropped stale `mira-runtime sidecar` wording; workflow renamed `Sync Latest RillML Runtime`; docs references updated | `rill-runtime` / `rill-runtime-protocol` / `sync-rill-runtime.yml` / `resolve-latest-rill-release.mjs` / `mira-rill-sync` — unchanged | CI/Release 32302279764 (workflow_dispatch): rill-release / model-pack / macOS+Windows bundle success; Linux bundle blocked by external apt-get (Azure mirror outage, non-branding); Sync Latest RillML Runtime 32293787911 success | VERIFIED |
| `hello-yunshu/mira-mouse-plugins` | — | UNAFFECTED | none (no RillML references) | — | — | UNAFFECTED |
| `hello-yunshu/homebrew-mira` | — | UNAFFECTED | none (no RillML references) | — | — | UNAFFECTED |
| `hello-yunshu/luci-app-performance-manager` | `7d1d60e` | REAL_DEPENDENCY / MODIFIED | README + README.en.md first-mention `RillML (Rill)`; `RillML upstream runtime` in architecture diagram; zh_Hans translation gate fixed | `performance-manager-rill` / `contracts/rill-dependency.json` / `pm-rill-shadow` / `rill-pm-adapter` / `config rill` / `rill_status` / `rill.sock` — unchanged | CI 32294828836 (static / pm-rill-provenance / openwrt-ucode success; pm-rill-runtime / pm-core-rill-roundtrip pre-existing rc.7 failures, non-branding); Build OpenWrt 32294828794 (see CI evidence) | VERIFIED |
| `hello-yunshu/rill-xray-agent` | `3709b02c` (PR #10 merged → main) | REAL_INTEGRATION / MODIFIED | stable status display unified to `RillML Xray Agent 0.1.0 (stable)` in README + README_EN; PACKAGE_SHA256SUMS refreshed | `rill-xray-agent` / `rill_payload` / `scripts/rill_xray_agent_*` / `systemd/rill-xray-agent-*` / `RillMLError` family — unchanged | PR #10 merged 2026-08-20; Source Gates + RillML remote consumer (gnu/musl) + PID1 all pass (runs 32295317255 / 32295317272 / 32295321530 / 32295321531) | VERIFIED |
| `hello-yunshu/Xray_bash_onekey` | `455a93c9` (PR #83 merged → main) | REAL_INTEGRATION / MODIFIED | README + 5 i18n languages first-mention `RillML（简称 Rill）` / `RillML (Rill)`; PO/POT unchanged (menu strings are UI shorthand, §brand standard) | `rill-xray-agent` integration / `rill_payload` / `scripts/rill_xray_agent_*` / `systemd/rill-xray-agent-*` / `RillMLError` family — unchanged | PR #83 merged 2026-08-20 after green rerun; Rill Xray Agent 32293640162 / 32293643229 success; Test Install + release-gate rerun (run 32293643225) all success | VERIFIED |
| `hello-yunshu/lessonforge` | `27a87da` | REAL_DEPENDENCY / MODIFIED | `document-intelligence/pyproject.toml` description → `RillML Assist/MLOps 平台`; README first-mention `RillML` | `RillML IPC v3` / `Rill Shadow` / `DI_RILL_*` env names — unchanged | no CI workflow (docs-only change) | VERIFIED |
| `hello-yunshu/Xray_bash_onekey_api` | — | TECHNICAL_REFERENCE | none | `rill-ml` in `test_promote_validation.sh` as disallowed candidate-string fixture | — | UNAFFECTED |
| `hello-yunshu/github-hey-run` | — | TECHNICAL_REFERENCE | none | `rill-ml` in `src/config.js` app registry pointing to canonical repo | — | UNAFFECTED |
| 25 other owner repos | — | UNAFFECTED | none (no RillML terms) | — | — | UNAFFECTED |

## Classification summary

- OFFICIAL_BRAND / CANONICAL: `rill-ml`
- REAL_DEPENDENCY: `mira-mouse`, `luci-app-performance-manager`, `lessonforge`
- REAL_INTEGRATION: `rill-xray-agent`, `Xray_bash_onekey`
- TECHNICAL_REFERENCE: `Xray_bash_onekey_api`, `github-hey-run`
- UNAFFECTED: all remaining owner repositories

## Technical identifiers preserved (non-exhaustive, unchanged across all repos)

`rill-ml` · `rill_ml` · `rill-runtime` · `rill-runtime-protocol` · `rill-handler-api` · `rill-pm-adapter` · `rill-xray-agent` · `.rillpack` · `.rillhandler` · `rill:handler` · `RillMLError`/`RillMLUnsupported`/`RillMLValidationError`/`RillMLDownloadError`/`RillMLProbeError` · `rill_payload` · `scripts/rill_xray_agent_*` · `systemd/rill-xray-agent-*` · `performance-manager-rill` · `contracts/rill-dependency.json` · `pm-rill-shadow` · `rill_status` · `/run/performance-manager/rill.sock` · `sync-rill-runtime.yml` · `resolve-latest-rill-release.mjs` · `mira-rill-sync` · `mira-rill-2026-002` · `mira-handler-2026-001` · `rillml-examples-2026-001`

## Residual scan

See `RILLML_DOWNSTREAM_RESIDUAL_SCAN.json`. No active Rillun brand usage anywhere; the only `Rillun` hits are historical audit conclusions in `rill-ml/docs/audits/` documenting absence.
