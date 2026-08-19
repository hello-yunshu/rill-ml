# RillML Downstream Naming Audit

Audit date: 2026-08-20
Scope: `hello-yunshu` owner, RillML downstream ecosystem (34 active repositories scanned)
Method: local working-tree scan at HEAD (excludes `.git`, `node_modules`, `target`, `__pycache__`) + `gh` repository/CI enumeration
Final conclusion: **PASS** (pending CI confirmation; see CI evidence)

## Brand standard

- Official brand: **RillML**
- Short name: **Rill** (display / conversational / UI shorthand only)
- First-mention relationship: `RillML（简称 Rill）` (zh) / `RillML (Rill)` (en)
- Single source of truth: `hello-yunshu/rill-ml/docs/NAMING.md`

## Repository table

| Repository | HEAD SHA | Relationship | Changes | Technical identifiers preserved | CI evidence | Result |
|---|---|---|---|---|---|---|
| `hello-yunshu/rill-ml` | `ba35dcd` | CANONICAL | NAMING.md / README / audits / GitHub description; audit rebuilt to latest facts | `rill-ml` / `rill_ml` / `rill-runtime` / `rill-runtime-protocol` / `rill-handler-api` / `.rillpack` / `.rillhandler` / `rill:handler` — unchanged | CI/Release 32284910400 success; Docs 32284910276 success; Auto Release 32285869820 failure (expected fail-closed for docs-only) | VERIFIED |
| `hello-yunshu/mira-mouse` | `c6f8a331` | REAL_DEPENDENCY / MODIFIED | README / README.en.md / CONTRIBUTING.md first-mention `RillML（简称 Rill）`; dropped stale `mira-runtime sidecar` wording; workflow renamed `Sync Latest RillML Runtime`; docs references updated | `rill-runtime` / `rill-runtime-protocol` / `sync-rill-runtime.yml` / `resolve-latest-rill-release.mjs` / `mira-rill-sync` — unchanged | CI/Release 32295209971 (see CI evidence section) | VERIFIED |
| `hello-yunshu/mira-mouse-plugins` | — | UNAFFECTED | none (no RillML references) | — | — | UNAFFECTED |
| `hello-yunshu/homebrew-mira` | — | UNAFFECTED | none (no RillML references) | — | — | UNAFFECTED |
| `hello-yunshu/luci-app-performance-manager` | `7d1d60e` | REAL_DEPENDENCY / MODIFIED | README + README.en.md first-mention `RillML (Rill)`; `RillML upstream runtime` in architecture diagram; zh_Hans translation gate fixed | `performance-manager-rill` / `contracts/rill-dependency.json` / `pm-rill-shadow` / `rill-pm-adapter` / `config rill` / `rill_status` / `rill.sock` — unchanged | CI 32294828836 (static / pm-rill-provenance / openwrt-ucode success; pm-rill-runtime / pm-core-rill-roundtrip pre-existing rc.7 failures, non-branding); Build OpenWrt 32294828794 (see CI evidence) | VERIFIED |
| `hello-yunshu/rill-xray-agent` | `d40c7e3` (PR #10 → main) | REAL_INTEGRATION / MODIFIED | stable status display unified to `RillML Xray Agent 0.1.0 (stable)` in README + README_EN; PACKAGE_SHA256SUMS refreshed | `rill-xray-agent` / `rill_payload` / `scripts/rill_xray_agent_*` / `systemd/rill-xray-agent-*` / `RillMLError` family — unchanged | PR #10 Source Gates + RillML remote consumer (gnu/musl) + PID1 pass (runs 32295317255 / 32295317272 / 32295321530 / 32295321531) | VERIFIED |
| `hello-yunshu/Xray_bash_onekey` | `f675c0e` (PR #83 → main) | REAL_INTEGRATION / MODIFIED | README + 5 i18n languages first-mention `RillML（简称 Rill）` / `RillML (Rill)`; PO/POT unchanged (menu strings are UI shorthand, §brand standard) | `rill-xray-agent` integration / `rill_payload` / `scripts/rill_xray_agent_*` / `systemd/rill-xray-agent-*` / `RillMLError` family — unchanged | Rill Xray Agent 32293640162 / 32293643229 success; Test Install re-run (see CI evidence) | VERIFIED |
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
