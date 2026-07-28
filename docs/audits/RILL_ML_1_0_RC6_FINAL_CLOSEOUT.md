# RillML 1.0 Final Closeout Report

**Audit date:** 2026-07-28

**Scope:** `RILL_ML_TRAE_RC6_FINAL_CLOSEOUT_UNSIGNED_MAC_PROMPT.md`

**Repository:** `hello-yunshu/rill-ml`

## Decision

`v1.0.0-rc.6` itself is healthy and its published runtime path works on a
real external host. The original closeout implementation nevertheless had
three rigor gaps: its host smoke accepted an invoke error as success and did
not trust the handler key, four v1 fixtures existed but were not loaded by
the cross-version test target, and documentation contradicted the permanent
unsigned-macOS policy.

The merged closeout repairs those gaps and additionally prevents a channel
pointer from moving until the immutable published assets pass an independent
host smoke. The complete local and remote closeout gates passed, and final
`1.0.0` was subsequently published successfully. Its immutable released
assets passed the independent host smoke before the signed Stable pointer
moved. Apple Developer ID signing remains an explicit non-blocking policy.

## Immutable RC6 identity

| Item | Value |
|---|---|
| `origin/main` before closeout repairs | `49d0155ff763f268082c91eae420aa31dacfc079` |
| `v1.0.0-rc.6^{commit}` | `49d0155ff763f268082c91eae420aa31dacfc079` |
| GitHub prerelease | `https://github.com/hello-yunshu/rill-ml/releases/tag/v1.0.0-rc.6` |
| Release run | `30343080148` — success |
| CI run | `30342441389` — success |
| Security run | `30342441602` — success |
| Docs run | `30342441465` — success |

The RC6 tag, release, and immutable assets were not moved or replaced.

## Stable and Preview boundary

The Stable group is:

- `rill-handler-api`
- `rill-runtime-protocol`
- `rill-ml`
- `rill-runtime`

All four are published as `1.0.0-rc.6` on crates.io. Preview crates remain at
`0.13.0`; `rill-ml-python` remains `0.13.0` on PyPI and is not a 1.0 blocking
artifact.

For serde state, only the 26 types in `state-schema-manifest.toml` and
`STABILITY.md` are covered by the 1.x state-schema freeze. The 12 explicitly
listed Preview state types remain serializable without a cross-1.x
compatibility promise.

## State fixture and malicious-state gates

The hardened coverage script now verifies all of the following:

- the Stable and Preview manifest groups have no duplicates or overlap;
- all 26 Stable types have both `v0.13.0` and `v1` fixture files;
- both generations are referenced by concrete test functions;
- every Stable type has its declared validation mechanism;
- the ordered Stable and Preview lists in `STABILITY.md` exactly match the
  manifest.

The cross-version target runs 26 `v0.13.0` loads and 26 `v1` loads. The
previously unexercised v1 fixtures are `optimizer_sgd`, `regression_loss`,
`sparse_features`, and `feature_hasher`.

The malicious-state suite additionally proves rejection of:

- inconsistent encoder category/index ordering;
- bandit per-arm vectors that do not match `arm_count`;
- a fixed-window drift buffer whose `len` exceeds `capacity`.

`FixedWindowBuffer` now uses a validating custom `Deserialize` implementation
for persisted-state trust boundaries while remaining a Preview state schema.
This tightens malformed-input handling without changing the frozen public API.

## Independent released-asset host smoke

Host: `external-host-smoke`

Host implementation: `smoke-test/host_smoke.py`

Host source commit recorded in the structured log:
`49d0155ff763f268082c91eae420aa31dacfc079`

Platform exercised locally: macOS aarch64

The host imports no Rill crate or internal Python module. It:

1. downloads the public candidate pointer index;
2. verifies its Ed25519 signature using the public `rill-pack` CLI;
3. selects the current OS/architecture runtime plus the RC6 model and handler;
4. verifies every downloaded size and SHA-256 against the signed index;
5. verifies both package signatures;
6. launches the runtime with separate model and handler trust keys;
7. requires v2 handshake identities and non-empty effective capabilities;
8. requires `health.healthy == true`;
9. requires a real `kind: "result"` invoke response;
10. verifies the frozen `invalidJson` error code;
11. requires a clean EOF shutdown.

Result on the published RC6 assets: **12/12 checks passed**.

Published artifact evidence:

| Artifact | SHA-256 | Bytes |
|---|---|---:|
| `rill-runtime-1.0.0-rc.6-linux-x86_64` | `59752af7e0a15f5fbe0a13924abf80188555beda4edd18baa99bb3258122fc74` | 19035144 |
| `rill-runtime-1.0.0-rc.6-macos-aarch64` | `d91e983bc402de82dbb41230625d82082279f3a6720885ae2891a6b8d7219a67` | 13901248 |
| `rill-runtime-1.0.0-rc.6-windows-x86_64.exe` | `0660d28d3ead6c3d8ed23a7c7555814ed6a415ce9f4562f1e9fae65c3cd0d364` | 14404608 |
| `example-default-1.0.0-rc.6.rillpack` | `9744f72d3902fbec7ed2dfe0a6d9b3efe5a0bd508aaf4caf84cc1547b4343ab5` | 928 |
| `echo-handler-1.0.0-rc.6.rillhandler` | `9a69d2356cc635dae5259c0eee7a58236d620bf41efe890df1880e2cb0a0c25c` | 8845 |

The old script's failed log was replaced by the successful structured
evidence at `smoke-test/smoke-result.json`.

## Release workflow hardening

For future candidate and stable releases:

- the release notes explicitly describe unsigned/non-notarized macOS delivery;
- the immutable versioned release is smoked from its public release URL;
- the smoke JSON is uploaded as an Actions artifact even on failure;
- the candidate or stable pointer moves only after the smoke succeeds;
- the immutable index is downloaded and signature-verified again immediately
  before pointer promotion.

The signed release-index schema remains frozen. It records the macOS artifact,
size, and SHA-256. The separate `*.unsigned.json` release asset records
`codeSigning: "unsigned"` and `notarization: false`.

## Formal-promotion preflight findings

The `1.0.0` version bump exposed two additional release-path problems before
any immutable tag or registry upload:

- the excluded echo and malicious-handler lockfiles still recorded RC6 after
  their manifests advanced, allowing a stale locally generated echo component
  to report RC6 metadata against a `1.0.0` manifest;
- the package-check job attempted to package every Preview workspace crate.
  Those Preview crates depend on the not-yet-published `rill-ml 1.0.0`, so a
  first-attempt final release would fail before publishing. RC retries could
  mask this because their internal versions might already be indexed.

The version synchronizer now updates both excluded-handler lockfiles and has a
regression test for named-package-only lock updates. The package-check job is
now driven by the Stable list and paths in `release-plan.toml`; it verifies all
four Stable archives with all features while patching unpublished internal
dependencies to the exact checked-out tag. Preview crates are excluded. The
repaired package path and `rill-ml` crates.io publish dry-run pass locally.

## macOS delivery policy

Official macOS runtime assets are Apple Silicon only. Missing Apple Developer
ID credentials never skip build, stage, upload, index inclusion, or release.
The default official asset is unsigned and not notarized; this is an accepted
delivery form for RC and final releases. Users may authorize first launch
through Finder → Open or System Settings → Privacy & Security. Model,
handler, and release-index cryptographic verification remains mandatory.

## Channel state before final promotion

| Channel | Pointer | Version |
|---|---|---|
| stable | `local-ai-stable/stable-index.json` | `0.13.0` |
| candidate | `local-ai-candidate/candidate-index.json` | `1.0.0-rc.6` |

RC6 did not modify the stable channel.

## Final 1.0.0 promotion

Promotion to `1.0.0` requires:

1. all local formatting, lint, test, fixture, API, WIT, release-script, and
   documentation gates to pass on the repaired tree;
2. the closeout change to pass GitHub CI, Security, and Docs;
3. no SemVer-breaking change from the RC baseline;
4. the final release workflow's independent published-asset smoke to pass
   before `local-ai-stable` is advanced;
5. post-release verification of the tag, GitHub assets, crates.io packages,
   signed stable index, pointer contents, and smoke artifact.

Closeout repair PR `#17` merged as
`dacf7f2e30aed637917b874857ee47098342f2af`. Its PR checks and the subsequent
`main` runs all passed:

- CI / Release: `30347458228`
- Security audit: `30347458229`
- Docs: `30347458314`

The formal promotion PR `#18` passed its complete PR matrix and merged as
`b726543e19fd610736739cefac89e0ecb101e5b2`:

- PR CI / Release: `30349428060`
- PR Security audit: `30349427891`
- PR Docs: `30349427940`
- `main` CI / Release: `30350178707`
- `main` Security audit: `30350178724`
- `main` Docs: `30350178705`

Auto Release run `30350650510` confirmed that the successful CI commit was
still `main`, validated version `1.0.0`, created the annotated `v1.0.0` tag
at the merge commit using `git tag -a`, and dispatched the tagged release.

Release run `30350663967` completed successfully. Its authoritative gates
proved all of the following:

- all four Stable crate archives packaged and verified successfully;
- the `rill-ml` crates.io publish request passed before publication;
- `rill-handler-api`, `rill-runtime-protocol`, `rill-ml`, and `rill-runtime`
  were published in the order declared by `release-plan.toml`;
- Linux x86_64, Windows x86_64, and macOS aarch64 runtime assets built and
  uploaded;
- the macOS aarch64 asset followed the accepted unsigned/non-notarized path
  and emitted the unsigned-policy annotation;
- the model, handler, and Stable index were signed and verified before the
  immutable GitHub Release was created;
- the independent released-asset host smoke downloaded and exercised the
  public runtime, model, and handler successfully, then uploaded
  `released-asset-host-smoke-1.0.0`;
- the immutable Stable index was downloaded and signature-verified again
  before `local-ai-stable/stable-index.json` was replaced;
- the Python publish gate detected the independent Preview version `0.13.0`
  and correctly skipped PyPI publication.

The release workflow therefore satisfied promotion requirements 1–5 in the
required order. The post-promotion channel state is:

| Channel | Pointer | Version |
|---|---|---|
| stable | `local-ai-stable/stable-index.json` | `1.0.0` |
| candidate | `local-ai-candidate/candidate-index.json` | `1.0.0-rc.6` |

## Final decision

**RillML `1.0.0` is formally released.** There is no known RC6 closeout,
state-schema, public-API, SemVer, packaging, registry, runtime-asset,
signature, host-smoke, or channel-promotion blocker.

The only intentional delivery caveat is the permanent macOS policy: the
official Apple Silicon runtime is unsigned and not notarized when Apple
Developer ID credentials are absent. That status is explicit in the workflow,
release notes, documentation, and sidecar metadata and does not weaken the
mandatory Ed25519 verification of the model, handler, or release index.
