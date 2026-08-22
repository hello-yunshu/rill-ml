# RillML 1.3.0 `before` audit

Date: 2026-08-22 (Asia/Shanghai)

This is the frozen baseline for the independent RillML 1.3.0 execution prompt.
It records facts observed before the 1.3 implementation changes. Remote GitHub
facts were read from the repository and Actions API on the date above; they are
not inferred from local documentation.

## Repository and release baseline

| Field | Observed baseline |
|---|---|
| Default branch | `main` |
| Local/origin HEAD | `93e243bfacaffe9d2ce1ed80d4f78fbe20b25ce9` |
| Latest Stable release | `v1.2.0` |
| Stable tag target | `0087964f8599f86df6531d1106da72c55c8b8ae1` |
| Stable release URL | <https://github.com/hello-yunshu/rill-ml/releases/tag/v1.2.0> |
| Release index pointer | `local-ai-stable`, with `stable-index.json` |
| Release-index schema | v3 (`targetLibc` distinguishes Linux GNU/musl) |
| Workspace Stable group | `rill-handler-api`, `rill-runtime-protocol`, `rill-ml`, `rill-runtime` at `1.2.0` |
| Workspace Preview group | `rill-ml-python`, `rill-ml-wasm`, `rill-ml-tokio`, `rill-ml-arrow`, `rill-ml-polars`, `rillml-inspect`, `rill-ml-ffi`, `rill-pm-adapter` at `0.15.0` |
| MSRV | Rust `1.94` / Edition 2024 |
| Stable IPC | v1 and v2; `RUNTIME_API_VERSION = 2` |
| Stable handler ABI | WIT/handler ABI v1 |
| Stable pack formats | `.rillpack` v1 and `.rillhandler` v1 |
| Stable persisted state | Snapshot format v1 plus the manifest-listed v0.13.0/v1 fixtures |
| Preview runtime surface | IPC v3 and Stateful Handler ABI v2 library types; no production CLI v3 entrypoint |
| Open issues | 0 observed |
| Open pull requests | 0 observed |

## Latest remote CI and release state

| Workflow/run | SHA | Result | Evidence |
|---|---|---|---|
| CI / Release `32304744835` | `93e243b` | PASS | <https://github.com/hello-yunshu/rill-ml/actions/runs/32304744835> |
| Docs `32304744808` | `93e243b` | PASS | <https://github.com/hello-yunshu/rill-ml/actions/runs/32304744808> |
| Auto Release `32305374657` | `93e243b` | FAIL | Security Audit same-SHA gate timed out/failed; <https://github.com/hello-yunshu/rill-ml/actions/runs/32305374657> |
| Latest Security Audit for this SHA | `93e243b` | NOT_RUN | The latest listed Security Audit was for `61d7256`; it did not cover this docs-only commit. |

The release gate therefore did not create a 1.3 candidate or release. The
failure is a current release-automation observation, not evidence that the
security code itself failed.

## v1.2.0 release assets

The stable Release contained 19 assets: two signed packages
(`example-default-1.2.0.rillpack`, `echo-handler-1.2.0.rillhandler`), 14
runtime/adapter binaries across Linux, FreeBSD, macOS and Windows, one macOS
unsigned sidecar, and `stable-index.json`. It did not contain an SBOM or a
machine-readable consumer-conformance report.

## Product-surface baseline

| Surface | Observed behavior | Gate status |
|---|---|---|
| Stable IPC | Runtime server parses the shared v1/v2 `RuntimeRequest`; v1/v2 response schemas are separate and frozen. | Existing coverage; re-run in 1.3 |
| Preview IPC v3 | `rill-runtime-protocol::v3` and `StatefulRuntimeEngineV3` exist as library surfaces. The production `rill-runtime serve` command has no v3 subcommand/flag. | Documentation must explicitly say library-only Preview/not exposed by production CLI |
| Handler fallback | `serve` without `--handler` and without `--builtin-handler` returns `MissingHandlerOption`; `--builtin-handler linear-regression` is an explicit deprecated path. | Docs drift: fix Chinese and English product surface |
| Signed packs/index | Archive path, size, compression, checksum, signature and independent model/handler trust checks exist; release index schema v3 is validated. | Re-run negatives and add lifecycle policy |
| Trust store | `TrustStore` is a multi-key `BTreeMap<keyId, VerifyingKey>`, but has no validity window, revocation, emergency revoke or rollback metadata. | 1.3 lifecycle work required |
| Consumer adoption | No authoritative adoption manifest is present. | Add schema, validator, example fixture and fail-closed unknown-version tests |
| Conformance | Existing release/index and host-smoke scripts are repository-specific; no reusable deterministic PASS/FAIL/BLOCKED/NOT_RUN kit is present. | 1.3 kit required |
| Fuzzing | No dedicated `fuzz/` Cargo LibFuzzer harness/corpus was present. | 1.3 harness required |
| SBOM | No CycloneDX/SPDX release asset or generator was present. | 1.3 generator/release wiring required |
| Actions pinning | Workflows use major tags/branches such as `actions/checkout@v7`, `dtolnay/rust-toolchain@stable/master`, and `Swatinem/rust-cache@v2`; only selected actions use commit SHAs. | Must pin only to verified real commits; no SHA may be guessed |

## Frozen compatibility evidence to re-run

- `api-baseline/` and Stable `cargo-semver-checks`/public API checks.
- `tests/fixtures/state/v0.13.0/`, `tests/fixtures/state/v1/`, and portable
  detector fixtures.
- Stable Runtime IPC v1/v2 fixtures and malformed/oversized request tests.
- `.rillpack`/`.rillhandler` signed and negative archive fixtures.
- WIT ABI check and FFI C smoke/opaque-handle tests.
- Release-plan/version/index helper tests and same-SHA release admission.

## 1.3 decision recorded before implementation

RillML 1.3 will keep IPC v3 as **library-only Preview**. It will not add a
production CLI v3 subprocess entrypoint in this release, and it will not change
the Stable `RUNTIME_API_VERSION = 2` meaning. Consumer adoption manifests remain
consumer-owned; this repository provides only their schema, validator, fixtures
and conformance tooling.
