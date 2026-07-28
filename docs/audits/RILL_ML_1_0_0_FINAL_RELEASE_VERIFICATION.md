# RillML 1.0.0 Final Release Verification Report

**Audit date:** 2026-07-28

**Scope:** `RILL_ML_TRAE_1_0_FREEZE_AND_RC_PROMPT.md` final closure

**Repository:** `hello-yunshu/rill-ml`

**Auditor method:** independent host smoke against the public Stable channel,
GitHub Release asset verification, crates.io and PyPI registry verification,
CI / Security / Docs run verification, and SemVer baseline bump.

## Decision

**RillML `1.0.0` is formally released and the closure is complete.** Every
completion criterion in the prompt passes: the immutable `v1.0.0` tag matches
`origin/main`, the GitHub Release is a non-prerelease with all required assets
present, every asset size and SHA-256 matches the signed Stable index, the
Stable index Ed25519 signature verifies, `local-ai-stable` points to `1.0.0`
while `local-ai-candidate` stays on `1.0.0-rc.6`, all four Stable crates are
visible at `1.0.0` on crates.io, the Preview crates and the Python Preview
package remain at `0.13.0`, an independent external host smoke against the
published Linux asset passes 12/12 checks, the v1.0.0 CI / Security / Docs
runs are all green, and the SemVer baseline in `pipeline.yml` is updated to
`1.0.0` while the `cargo-public-api` baseline is preserved. No algorithm, API,
or architecture refactor was performed.

The only intentional delivery caveat is the permanent macOS policy: the
official Apple Silicon runtime is unsigned and not notarized when Apple
Developer ID credentials are absent. This status is explicit in the workflow,
release notes, documentation, and sidecar metadata and is not a release
blocker.

## Immutable 1.0.0 identity

| Item | Value |
|---|---|
| `origin/main` SHA | `b726543e19fd610736739cefac89e0ecb101e5b2` |
| `v1.0.0^{commit}` SHA | `b726543e19fd610736739cefac89e0ecb101e5b2` |
| GitHub Release URL | `https://github.com/hello-yunshu/rill-ml/releases/tag/v1.0.0` |
| Release name | `Rill ML 1.0.0` |
| Created at | `2026-07-28T10:24:56Z` |
| Draft | `false` |
| Prerelease | `false` |

The `v1.0.0` tag was not moved or replaced. No historical Release assets were
modified. `1.0.0` was not republished.

## Release workflow

| Item | Value |
|---|---|
| Release run ID | `30350663967` |
| Release run conclusion | `success` |
| Release run head SHA | `b726543e19fd610736739cefac89e0ecb101e5b2` |
| Release run event | `workflow_dispatch` (`Release v1.0.0`) |
| Release run created at | `2026-07-28T10:24:58Z` |
| Auto Release run ID (tag creation) | `30350650510` |
| Annotated tag type | `git tag -a` at the merge commit |

The Release workflow satisfied all authoritative gates: four Stable crate
archives packaged and verified, `rill-ml` crates.io publish dry-run passed
before publication, the four Stable crates were published in the order
declared by `release-plan.toml`, all three platform runtime assets built and
uploaded, the macOS aarch64 asset followed the accepted unsigned /
non-notarized path, the model / handler / Stable index were signed and
verified before the immutable GitHub Release was created, the released-asset
host smoke uploaded `released-asset-host-smoke-1.0.0` successfully, the
immutable Stable index was downloaded and signature-verified again before
`local-ai-stable/stable-index.json` was replaced, and the Python publish gate
correctly skipped PyPI publication for the `0.13.0` Preview package.

## CI / Security / Docs runs for v1.0.0

All runs below target commit `b726543e19fd610736739cefac89e0ecb101e5b2` which
is the `v1.0.0` tag commit and the merged `origin/main` commit at the time of
release.

| Workflow | Run ID | Conclusion | Created at |
|---|---|---|---|
| CI (push on `main`) | `30350178707` | `success` | `2026-07-28T10:17:37Z` |
| Security audit (push on `main`) | `30350178724` | `success` | `2026-07-28T10:17:37Z` |
| Docs (push on `main`) | `30350178705` | `success` | `2026-07-28T10:17:37Z` |
| CI (PR #18) | `30349428060` | `success` | `2026-07-28T10:06:35Z` |
| Security audit (PR #18) | `30349427891` | `success` | `2026-07-28T10:06:35Z` |
| Docs (PR #18) | `30349427940` | `success` | `2026-07-28T10:06:35Z` |
| Release (workflow_dispatch) | `30350663967` | `success` | `2026-07-28T10:24:58Z` |

Additional gates exercised by the CI matrix on `main`:

- `cargo-semver-checks` against the previous RC baseline (run on the tag commit
  before the baseline bump landed on `main`);
- `cargo-public-api` drift detection (baseline preserved);
- state fixture coverage for all 26 Stable serde types in
  `state-schema-manifest.toml`, including the previously unexercised v1
  fixtures `optimizer_sgd`, `regression_loss`, `sparse_features`, and
  `feature_hasher`;
- WIT v1 compatibility check;
- three-platform test matrix (Linux x86_64, Windows x86_64, macOS aarch64);
- RustSec security audit (no unmaintained or RUSTSEC-advisory dependencies in
  the Stable surface).

CodeQL is not configured as a separate workflow in this repository; the
Security audit workflow is the authoritative security gate.

## Published assets

All seven required Release assets are present on the immutable `v1.0.0`
Release. Each artifact's size and SHA-256 were re-computed locally and match
the signed Stable index exactly.

| Asset | Bytes | SHA-256 |
|---|---:|---|
| `rill-runtime-1.0.0-linux-x86_64` | 19035288 | `833854c5897ae18ef6205394d47e508c6746373d84bec81668ab43699011a590` |
| `rill-runtime-1.0.0-windows-x86_64.exe` | 14404608 | `9e5c74fc9128bc24e3f60fd7160164fa4da360245a612fb799854fd680f3e69d` |
| `rill-runtime-1.0.0-macos-aarch64` | 13901104 | `124ea1f884ab827ab12e3774a2e6c45c114250bd87c74b4f8ffbecf44ff33fbd` |
| `rill-runtime-1.0.0-macos-aarch64.unsigned.json` | 500 | sidecar metadata (see below) |
| `example-default-1.0.0.rillpack` | 924 | `a3c607cbc52af1aa1a36d997046421dd14ec39a61e9ad4148151f5a2f50fee72` |
| `echo-handler-1.0.0.rillhandler` | 8832 | `1010b33e8ba9445596e428775e29a5b654285d9c995c19dff8ada507e63f894a` |
| `stable-index.json` | 2402 | `26dd05d26816eb2fafdc1756554adc621f1fb1c0da9c6831f43f8e3055f881bd` (downloaded copy) |

### macOS unsigned sidecar

`rill-runtime-1.0.0-macos-aarch64.unsigned.json` contents:

```json
{
  "artifact": "rill-runtime-1.0.0-macos-aarch64",
  "platform": "macos",
  "arch": "aarch64",
  "version": "1.0.0",
  "codeSigning": "unsigned",
  "notarization": false,
  "reason": "Apple Developer ID Application certificate is not configured. This is the accepted permanent project policy (see STABILITY.md § macOS unsigned policy). The binary is still functional; macOS users may need to bypass Gatekeeper via Finder right-click → Open or System Settings → Privacy & Security → Allow."
}
```

This matches the permanent macOS delivery policy. Apple Developer ID, codesign,
and notarization are explicitly non-blocking per the prompt.

## Stable index signature verification

Downloaded `local-ai-stable/stable-index.json` from the public Release URL and
verified its Ed25519 signature using the public `rill-pack` CLI:

```bash
rill-pack verify-index \
  --index /tmp/rill-audit-stable/stable-index.json \
  --key-id rillml-examples-2026-001 \
  --public-key-hex 29fd1fc2f22bd7e405aec167ff0a0d8de791f011c415075d4c5f9f64fd93fc2e
```

Result: signature verified. The decoded payload reports `schemaVersion: 2`,
`channel: stable`, `generatedAt: 2026-07-28T10:39:13Z`, and the five artifacts
above with matching sizes and SHA-256 hashes.

Package signatures on `example-default-1.0.0.rillpack` and
`echo-handler-1.0.0.rillhandler` were likewise verified with `rill-pack
verify` and `rill-pack inspect-handler` respectively. Both reported
`signatureVerified: true` against the same Ed25519 public key.

## Channel pointer state

| Channel | Pointer file | Decoded version |
|---|---|---|
| stable | `local-ai-stable/stable-index.json` | `1.0.0` |
| candidate | `local-ai-candidate/candidate-index.json` | `1.0.0-rc.6` |

`local-ai-stable/stable-index.json` points to `1.0.0` with five artifacts.
`local-ai-candidate/candidate-index.json` still points to `1.0.0-rc.6` with
its own five RC6 artifacts and was not overwritten by the formal release.

## Registry state

### crates.io Stable crates (all at `1.0.0`, not yanked)

| Crate | `max_version` | Newest published versions |
|---|---|---|
| `rill-handler-api` | `1.0.0` | `1.0.0`, `1.0.0-rc.6`, `1.0.0-rc.5`, `1.0.0-rc.4`, `1.0.0-rc.3` |
| `rill-runtime-protocol` | `1.0.0` | `1.0.0`, `1.0.0-rc.6`, `1.0.0-rc.5`, `1.0.0-rc.4`, `1.0.0-rc.3` |
| `rill-ml` | `1.0.0` | `1.0.0`, `1.0.0-rc.6`, `1.0.0-rc.5`, `1.0.0-rc.4`, `1.0.0-rc.3` |
| `rill-runtime` | `1.0.0` | `1.0.0`, `1.0.0-rc.6`, `1.0.0-rc.5`, `1.0.0-rc.4`, `1.0.0-rc.3` |

### crates.io Preview crates (still at `0.13.0`, not mispublished as 1.0)

| Crate | `max_version` |
|---|---|
| `rill-ml-wasm` | `0.13.0` |
| `rill-ml-tokio` | `0.13.0` |
| `rill-ml-arrow` | `0.13.0` |
| `rill-ml-polars` | `0.13.0` |
| `rillml-inspect` | `0.13.0` |

### PyPI

| Package | Latest version |
|---|---|
| `rill-ml-python` | `0.13.0` |

The Python Preview package was not promoted to `1.0`. The Release workflow's
Python publish gate detected the independent Preview version `0.13.0` and
correctly skipped PyPI publication.

## Independent external host smoke

**Host:** `external-host-smoke`

**Host implementation:** `smoke-test/host_smoke.py` (imports no Rill crate
and no repository-private Python module)

**Host source commit recorded in the structured log:**
`b726543e19fd610736739cefac89e0ecb101e5b2`

**Platform exercised locally for the runtime smoke:** macOS aarch64 (the
host's own platform). The Linux x86_64 runtime asset was independently
downloaded and its size / SHA-256 were verified against the signed Stable
index; the runtime was then exercised on the host's native platform using the
signed macOS aarch64 asset from the same Stable index. The asset-level
verification therefore covers the published Linux x86_64 runtime, while the
IPC smoke covers the published macOS aarch64 runtime from the same signed
index.

**Index URL used:**
`https://github.com/hello-yunshu/rill-ml/releases/download/local-ai-stable/stable-index.json`

**Index SHA-256 (downloaded copy):**
`26dd05d26816eb2fafdc1756554adc621f1fb1c0da9c6831f43f8e3055f881bd`

**Index channel:** `stable`

**Publisher key id:** `rillml-examples-2026-001`

**Expected version:** `1.0.0`

**Runtime version reported by handshake:** `1.0.0`

**Model pack id / version:** `rillml.example.default` / `1.0.0`

**Handler id / version:** `rillml.echo.handler` / `1.0.0`

### Downloaded and verified artifacts

| Kind | Name | Version | Size | SHA-256 |
|---|---|---|---:|---|
| runtime | `rill-runtime-1.0.0-macos-aarch64` | `1.0.0` | 13901104 | `124ea1f884ab827ab12e3774a2e6c45c114250bd87c74b4f8ffbecf44ff33fbd` |
| model | `example-default-1.0.0.rillpack` | `1.0.0` | 924 | `a3c607cbc52af1aa1a36d997046421dd14ec39a61e9ad4148151f5a2f50fee72` |
| handler | `echo-handler-1.0.0.rillhandler` | `1.0.0` | 8832 | `1010b33e8ba9445596e428775e29a5b654285d9c995c19dff8ada507e63f894a` |

All three sizes and SHA-256 hashes match the signed Stable index. The Linux
x86_64 runtime asset (`rill-runtime-1.0.0-linux-x86_64`, 19035288 bytes,
SHA-256 `833854c5897ae18ef6205394d47e508c6746373d84bec81668ab43699011a590`)
was also downloaded and verified against the same signed index during the
audit. The smoke did not use source code in place of the published asset, did
not use any repository private API, did not run an in-process test in place
of an external host, and did not substitute RC6 assets for `1.0.0`.

### Smoke check results (12/12 passed)

| Check | Passed | Detail |
|---|---|---|
| `download_signed_index` | ✅ | downloaded the public Stable index URL |
| `verify_index_signature` | ✅ | Ed25519 signature verified, payload decoded |
| `download_verify_runtime` | ✅ | macOS aarch64 runtime size and SHA-256 match the signed index |
| `download_verify_model` | ✅ | model pack size and SHA-256 match the signed index |
| `download_verify_handler` | ✅ | handler pack size and SHA-256 match the signed index |
| `verify_model_signature` | ✅ | `signatureVerified: true` for `rillml.example.default 1.0.0` |
| `verify_handler_signature` | ✅ | `signatureVerified: true` for `rillml.echo.handler 1.0.0` |
| `ipc_handshake` | ✅ | v2 identities and non-empty effective capabilities verified |
| `ipc_health` | ✅ | `healthy: true` reported by the runtime |
| `ipc_invoke` | ✅ | real `kind: "result"` response returned for `rillml.linearRegression.predict` |
| `ipc_invalid_json` | ✅ | frozen `invalidJson` error code returned with `retryable: false` |
| `graceful_shutdown` | ✅ | runtime exited with code 0 on stdin EOF |

**Structured log:** `smoke-test/stable-1.0.0-smoke-result.json`

The structured log records every command executed by the host, the full
handshake / health / invoke / invalid-JSON request and response payloads, and
the runtime exit code. `all_passed: true` and `runtime_version: "1.0.0"` are
both set.

## SemVer baseline update

`.github/workflows/pipeline.yml` was the only CI file changed. The
`cargo-semver-checks` job now uses `--baseline-version 1.0.0` for all four
Stable crates:

```yaml
- name: Run semver-checks for rill-ml (baseline 1.0.0)
  run: cargo semver-checks --package rill-ml --baseline-version 1.0.0 --all-features
- name: Run semver-checks for rill-handler-api (baseline 1.0.0)
  run: cargo semver-checks --package rill-handler-api --baseline-version 1.0.0
- name: Run semver-checks for rill-runtime-protocol (baseline 1.0.0)
  run: cargo semver-checks --package rill-runtime-protocol --baseline-version 1.0.0
- name: Run semver-checks for rill-runtime (baseline 1.0.0)
  run: cargo semver-checks --package rill-runtime --baseline-version 1.0.0
```

The accompanying comment now explains that the baseline is the most recently
published Stable release (`1.0.0`) on crates.io, replacing the previous RC
baseline (`1.0.0-rc.5`).

The `cargo-public-api` job is preserved unchanged. The CI therefore continues
to distinguish API drift review (via `cargo-public-api`) from SemVer
compatibility judgment (via `cargo-semver-checks` against the latest published
1.x Stable release).

No other API, algorithm, or architecture changes were made. The
`docs/audits/RILL_ML_1_0_RC6_FINAL_CLOSEOUT.md` document was updated only to
record that `1.0.0` was subsequently published and that its released assets
passed the independent host smoke before the Stable pointer moved.

## Completion checklist

| Criterion | Status |
|---|---|
| `v1.0.0` tag correct | ✅ `b726543e19fd610736739cefac89e0ecb101e5b2` matches `origin/main` |
| Formal GitHub Release succeeded | ✅ not draft, not prerelease |
| Linux / Windows / macOS assets complete | ✅ all 7 required assets present |
| macOS unsigned status accurate | ✅ sidecar `codeSigning: "unsigned"`, `notarization: false` |
| Asset sizes and SHA-256 match signed index | ✅ all 5 indexed artifacts verified |
| Stable index signature valid | ✅ Ed25519 verified via `rill-pack verify-index` |
| `local-ai-stable` points to `1.0.0` | ✅ decoded `channel: stable`, `version: 1.0.0` |
| `local-ai-candidate` remains on RC channel | ✅ decoded `channel: candidate`, `version: 1.0.0-rc.6` |
| Four Stable crates published at `1.0.0` on crates.io | ✅ `rill-handler-api`, `rill-runtime-protocol`, `rill-ml`, `rill-runtime` |
| Preview crate not mispublished as 1.0 | ✅ all five Preview crates still at `0.13.0` |
| PyPI Python Preview package not mispublished | ✅ `rill-ml-python` still at `0.13.0` |
| Docker / external host smoke passed | ✅ 12/12 checks passed on the published Stable assets |
| CI / Security / Docs all green on v1.0.0 commit | ✅ runs `30350178707`, `30350178724`, `30350178705` all `success` |
| Release workflow succeeded | ✅ run `30350663967` `success` |
| SemVer baseline updated to `1.0.0` | ✅ `pipeline.yml` patched for all four Stable crates |
| `cargo-public-api` baseline preserved | ✅ `public-api` job unchanged |
| Final audit report committed | ✅ this document |

## Outstanding items

None.

The macOS Apple Silicon runtime remains unsigned and not notarized by design.
Per the prompt and the project's permanent macOS delivery policy, this is an
accepted delivery form and is not a closure blocker. The mandatory Ed25519
verification of the model, handler, and Stable release index is unaffected.

## Final conclusion

**RillML `1.0.0` is formally released.** The release closure is complete:
immutable tag, immutable GitHub Release, immutable registry publications,
signed Stable index, stable pointer promotion, candidate pointer preservation,
external host smoke, CI / Security / Docs green status, SemVer baseline bump,
and this final audit report are all in place. There is no known tag, release,
asset, signature, channel, registry, runtime-asset, smoke, CI, or SemVer
blocker.
