# RillML 1.0 Stability Policy

This document defines the compatibility surface that RillML 1.x commits to
preserve. It is the single source of truth for what is stable, what is
preview, and how each artifact may evolve.

## Stability matrix

| Artifact | 1.0 status | Stable commitment |
|---|---|---|
| `rill-ml` | Stable | Rust public API; selected serde model state types listed below |
| `rill-runtime-protocol` | Stable | IPC v1/v2 JSON wire schema, release-index schema |
| `rill-handler-api` | Stable | WIT ABI v1, handler ABI constants |
| `rill-runtime` | Stable | Rust public API, CLI, model/handler pack loading |
| `rill-ml-python` | Preview | Python bindings; not covered by the 1.x API freeze |
| `rill-ml-wasm` | Preview | JS/WASM bindings; not covered by the 1.x API freeze |
| `rill-ml-tokio` | Preview | Rust adapter; not covered by the 1.x API freeze |
| `rill-ml-arrow` | Preview | Rust adapter; not covered by the 1.x API freeze |
| `rill-ml-polars` | Preview | Rust adapter; not covered by the 1.x API freeze |
| `rillml-inspect` | Preview (Tooling) | CLI output and arguments; not covered by the 1.x API freeze |

### Rationale for the split

The four Stable crates form the contracts that hosts, handler authors, and
downstream Rust applications compile against. Freezing them lets the runtime,
protocol, handler ABI, and core learning library evolve independently within
1.x.

The Preview crates are adapters and bindings whose surfaces are small but
still iterating. Keeping them at `0.x` avoids forcing a Python wheel matrix,
WASM JS fixture freeze, and adapter API baseline into the 1.0 RC admission
gate. They still ship from this repository and keep their existing security
and test quality, but they do not carry the 1.x no-breaking-change promise.

Preview crates depend on Stable crates via path dependencies. A `0.x` crate
depending on a `1.x` crate is valid in Cargo.

## Rust public API (Stable crates)

The public Rust API of each Stable crate is frozen by an API baseline stored
under `api-baseline/` and enforced by `cargo-semver-checks` in CI.

### `#[non_exhaustive]` policy

Enums and config structs are classified into two groups.

**Non-versioned, extensible types** carry `#[non_exhaustive]` so future
variants or fields can be added without a breaking change:

- `RillError`
- `Optimizer`
- `RegressionLoss`
- `HandlerLoadError`
- `ModelPackError`
- `HandlerPackError`
- `ArchiveError`
- `ReleaseIndexError`
- `InvokeErrorKind`
- `EngineResponse`
- `DriftAction`
- `DriftLevel`
- `WarmupState`
- `Confidence`
- `NewFeaturePolicy`

Application-facing config structs that may gain new optional fields also
carry `#[non_exhaustive]` and provide a `Default` or constructor.

**Versioned wire-schema types** (e.g. `RuntimeRequest`, `RuntimeResponse`,
`RuntimeResponseV2`) are NOT marked `#[non_exhaustive]`. They evolve by
introducing a new versioned type (e.g. `RuntimeResponseV3`), never by
appending variants or fields to an existing frozen type.

### Config strategy

Config structs that are expected to grow carry `#[non_exhaustive]` and a
`Default` implementation. Examples and docs use `Default::default()` or the
provided constructors rather than full struct literals where the field set
may expand.

### Public panic policy

Stable public APIs do not panic on ordinary user input. `Mutex::lock()`
calls in public paths return `Result` rather than `.expect()`. Internal
invariant panics are not reachable from external input. Rustdoc `# Panics`
sections document any remaining condition.

## serde model state

### Version model

`Snapshot<T>` carries an outer `format_version` (currently `1`). Each
Stable state schema type listed in the whitelist below implements
`ValidateState` (or an equivalent custom `Deserialize` / derive-based
validator) to enforce type-specific invariants (dimensions, finite values,
non-negative counts, optimizer parameter counts, FTRL `max_features`,
encoder mapping consistency, pipeline dimensions, bandit arm state).

Preview state schema types are serializable but their state schema is not
guaranteed to be compatible across 1.x versions.

### Restore contract

- `Snapshot::into_model()` checks the outer `format_version` only. It is
  safe for trusted state produced by the same code.
- `Snapshot::into_model_with_validation(validate)` checks the version and
  runs the caller-supplied validator before returning the model. This is the
  required path at trust boundaries.
- Python and WASM `from_json` go through validated restore and enforce a
  byte-size limit (`MAX_SNAPSHOT_JSON_BYTES`) before deserialization.

### Cross-version fixtures

Golden state fixtures live under `tests/fixtures/state/`:

- `v0.13.0/` — representative state produced by the immutable `v0.13.0` tag.
- `v1/` — state produced by the 1.0 RC candidate code.

RC and all future 1.x CI must load every fixture for Stable state schema
types. A schema bump requires a migration note in `CHANGELOG.md` and a new
fixture set.

The same golden fixtures are the cross-architecture stability gate: they are
loaded and byte-compared on every supported CPU architecture (native and
Docker + QEMU), so a state written on one architecture restores identically
on another. New stable state DTOs must use fixed-width integers
(`u32`/`u64`/`i32`/`i64`) only. See `PLATFORM_SUPPORT.md` →
*Cross-Architecture State Compatibility*.

### Stable state schema types

The following 26 types are covered by the 1.x state-freeze contract. Each
has full cross-version fixture coverage (`v0.13.0/` + `v1/`) and implements
state validation via one of three modes:

- `validate_state` — implements `ValidateState` trait.
- `deserialize` — custom `Deserialize` impl enforces invariants during
  deserialization.
- `derive` — serde-derived `Deserialize` for simple enums that automatically
  rejects unknown variants.

| Type | Fixture | Validation |
|---|---|---|
| `Mean` | `mean` | `validate_state` |
| `Variance` | `variance` | `validate_state` |
| `ExponentiallyWeightedMean` | `ew_mean` | `validate_state` |
| `StandardScaler` | `standard_scaler` | `validate_state` |
| `OneHotEncoder` | `one_hot_encoder` | `validate_state` |
| `OrdinalEncoder` | `ordinal_encoder` | `validate_state` |
| `FrequencyEncoder` | `frequency_encoder` | `validate_state` |
| `ConstantImputer` | `constant_imputer` | `validate_state` |
| `ForwardFill` | `forward_fill` | `validate_state` |
| `MeanImputer` | `mean_imputer` | `validate_state` |
| `MissingIndicator` | `missing_indicator` | `validate_state` |
| `FeatureHasher` | `feature_hasher` | `deserialize` |
| `SparseFeatures` | `sparse_features` | `deserialize` |
| `RegressionLoss` | `regression_loss` | `derive` |
| `LinearRegression` | `linear_regression` | `validate_state` |
| `MeanRegressor` | `mean_regressor` | `validate_state` |
| `FtrlClassifier` | `ftrl_classifier` | `validate_state` |
| `FtrlRegressor` | `ftrl_regressor` | `validate_state` |
| `GaussianNaiveBayes` | `gaussian_naive_bayes` | `validate_state` |
| `BernoulliNaiveBayes` | `bernoulli_naive_bayes` | `validate_state` |
| `MultinomialNaiveBayes` | `multinomial_naive_bayes` | `validate_state` |
| `Sgd` | `optimizer_sgd` | `validate_state` |
| `EpsilonGreedy` | `epsilon_greedy` | `validate_state` |
| `Ucb1` | `ucb1` | `validate_state` |
| `ThompsonSampling` | `thompson_sampling` | `validate_state` |
| `LinUcb` | `linucb` | `validate_state` |

### Preview state schema types

The following types are serializable but their state schema is not covered
by cross-version fixture coverage. Their state schema may change within
1.x without requiring a major version bump:

- `PageHinkley` (drift detector)
- `Adwin` (drift detector)
- `Kswin` (drift detector)
- `DriftAwareModel` (composite drift-aware model)
- `DriftEvent`
- `StaticStrategy`
- `TimeDecayedMean`
- `LearningRateScheduler`
- `FixedWindowBuffer`
- `LogisticRegression`
- `ExponentiallyWeightedMeanRegressor`
- `LastValueRegressor`
- `DriftConsensus`
- `P2Quantile`
- `P2Quantiles`
- `ClippedMean`
- `RollingMedianMad`
- `LinUcbFast`
- `DecisionLedger`
- `FeatureSchema`
- `ModelDescriptor`
- `WeightedMean`
- `WeightedVariance`
- `WeightedExponentiallyWeightedMean`
- `WeightedMae`
- `WeightedMse`
- `DecisionReplayHarness`

### State schema manifest

The authoritative source of truth for the Stable/Preview state schema
classification is `state-schema-manifest.toml`. The
`scripts/check_state_fixture_coverage.py` script validates in CI that:

- Every Stable type has a `v0.13.0` fixture.
- Every Stable type has a `v1` fixture.
- Every Stable type implements the declared validation mode.
- Fixture files exist on disk.
- No duplicate entries in either group.
- No overlap between Stable and Preview groups.

### Stable versioned portable detector state

The detector structs `PageHinkley`, `Adwin`, and `Kswin` retain their Preview
internal serde layouts. Stable continuity is instead provided by three
explicit versioned DTOs: `PageHinkleyPortableStateV1`,
`AdwinPortableStateV1`, and `KswinPortableStateV1`. Their `version = 1`
schemas, exact configuration matching, bounded windows, finite-value checks,
counter invariants, and golden fixtures under
`tests/fixtures/state/portable-v1/` are compatibility commitments. A future
breaking portable schema must introduce a new `V2` DTO; it must not mutate a
`V1` field or meaning.

`DriftConsensus`, `P2Quantile`, `P2Quantiles`, `ClippedMean`,
`RollingMedianMad`, `LinUcbFast`, `DecisionLedger`, the descriptor DTOs,
weighted statistics, and `DecisionReplayHarness` are additive Rust APIs whose
serialized implementation states remain Preview. `RollingMedianMad` is exact
only within its bounded FIFO window; it is not a lifetime-distribution state
contract. `LinUcbFast` does not change or replace the frozen `LinUcb` state
fields. Feature-schema hashes and their golden fixture are deterministic
identity checks, not a promise that every descriptor DTO field is frozen for
all 1.x releases.

IPC V3 and Stateful Handler ABI v2 are separate, opt-in Preview protocol
surfaces. They do not alter the frozen IPC v1/v2 JSON schemas, the top-level
`RUNTIME_API_VERSION = 2`, or the WIT v1 package/world/hash. Stateful v2 state
snapshots are runtime-owned and checksum-validated, but their ABI and snapshot
format are not yet Stable.

## IPC v1/v2

The v1 (`RuntimeResponse`) and v2 (`RuntimeResponseV2`) wire schemas are
permanently frozen for 1.x. New fields or semantics use a new versioned type
(e.g. `RuntimeResponseV3`), never an additive change to an existing type.

### Stable error codes

Error responses use the stable `error_code` constants defined in
`rill-runtime-protocol::error_code`. Existing codes are frozen:

- `invalidJson` — request body was not valid protocol JSON
- `invalidRequestId` — `requestId` was missing, empty, or too long
- `incompatibleApiVersion` — `apiVersion` was outside the supported range
- `invalidClientIdentity` — `clientName` / `clientVersion` failed validation
- `unsupportedCapability` — `Invoke` capability is not in the effective set
- `noInvokeHandler` — `Invoke` was issued but no handler is registered
- `handlerTimeout` — handler exceeded the wall-clock deadline (retryable)
- `handlerTrap` — handler trapped (unreachable, OOB, stack overflow, …)
- `handlerOutputTooLarge` — handler output exceeded the host-side size limit
- `handlerInvalidOutput` — handler output was not valid JSON
- `handlerInternalError` — handler reported an internal error (covers all
  four WIT `handler-error` variants on the wire for backwards compatibility)

New codes may be added (additive) but existing codes are never renamed.

### Protocol evolution

- v1/v2 types and wire schemas are permanently frozen.
- New required fields use a new versioned type.
- The runtime supports v1 and v2 throughout 1.x.
- Removing an old protocol version requires 2.0 or the end of the stated
  support period.

## WIT ABI v1

The WIT package `rill:handler@1.0.0` is frozen. `HANDLER_API_VERSION = 1`
and `WIT_VERSION = "1.0.0"` are enforced by `scripts/check_wit_abi.py`,
which verifies four independent declarations of the ABI version agree:

1. `crates/rill-handler-api/wit/rill-handler.wit` — the canonical WIT source.
2. `crates/rill-handler-api/src/lib.rs` — Rust constants
   (`HANDLER_API_VERSION`, `WIT_PACKAGE`, `WIT_VERSION`, `WIT_WORLD`).
3. `crates/rill-runtime-protocol/src/lib.rs` — the protocol constant
   `HANDLER_API_VERSION` used in handler-pack manifest validation.
4. SHA-256 of the normalised WIT text (frozen:
   `108a68dfd6bcf86e3b63ad630508b2bbf407d00e8634067366e53dbc257cc90c`).

A prebuilt v1 component fixture
(`crates/rill-runtime/tests/fixtures/handler-v1-component.wasm`) is committed
to the repository and loaded by `tests/wit_v1_component.rs` to prove the
current runtime still loads a component built from the `v0.13.0` tag using
the frozen WIT. CI does **not** rebuild this fixture from current WIT. The
fixture's SHA-256
(`6cfb4bf2eac5d0d5a4644c56f58b2fd679fc989893bc381a8ae31b410852011b`) is
verified before each load so silent replacement is detected.

### WIT evolution

- v1 types and the wire schema are permanently frozen. The frozen set
  includes: `handler-metadata`, `handler-error`, the `invoke-handler`
  world, and the three exported functions (`metadata`, `configure`,
  `invoke`).
- Additive changes within v1 are limited to new resources or new
  non-required interface functions; they must not break existing guests
  and must not change the normalised WIT hash without an explicit review
  that updates `FROZEN_WIT_SHA256` in `scripts/check_wit_abi.py`.
- Breaking changes (renaming or removing a type, changing a function
  signature, altering a variant's payload) require a new
  `rill:handler@2.x` package and a `HANDLER_API_VERSION` bump.
- The 1.x runtime continues to load v1 handlers for the entire 1.x cycle.
- `HANDLER_API_VERSION` and the WIT package major version are coupled:
  `rill:handler@1.x` ↔ `HANDLER_API_VERSION = 1`,
  `rill:handler@2.x` ↔ `HANDLER_API_VERSION = 2`, and so on.

## Model and handler packages

`.rillpack` (model pack) format version 1 and `.rillhandler` (handler pack)
format version 1 are frozen. `MODEL_PACK_FORMAT_VERSION = 1` and
`HANDLER_PACKAGE_FORMAT_VERSION = 1`. A format bump requires a migration
note and a new versioned manifest type.

## Runtime CLI

The `rill-runtime` CLI subcommands and arguments are frozen for 1.x:

- `serve` — `--pack`, `--model-trust-key` (primary), `--trust-key`
  (deprecated alias), `--handler-trust-key`, `--handler`,
  `--builtin-handler`.
- `inspect-pack` — `--pack`, `--model-trust-key`.
- `inspect-handler` — `--handler`, `--handler-trust-key`.

The `--builtin-handler linear-regression` path is retained as an explicit
compatibility option. The implicit fallback to the built-in handler when no
`--handler`/`--builtin-handler` is passed has been removed: the runtime
fails to start instead.

`rillml-inspect` CLI is Preview (Tooling) and may change.

## Default features

`rill-runtime` ships with `default = ["wasm"]` so that
`cargo install rill-runtime` matches the official GitHub binary behavior.
Builds that omit the feature must document that `.rillhandler` packs cannot
be loaded.

## MSRV

The Minimum Supported Rust Version for Stable crates is **1.94.0**,
enforced in CI. An MSRV bump is a minor breaking change and requires a
`CHANGELOG.md` note and a CI matrix update.

## Platform support

| Platform | Runtime | Core library |
|---|---|---|
| Linux x86_64 | Stable | Stable |
| Windows x86_64 | Stable | Stable |
| Windows ARM64 (aarch64) | Stable | Stable |
| macOS aarch64 (Apple Silicon) | Stable (unsigned) | Stable |
| macOS x86_64 (Intel) | Not published | Stable |

### macOS unsigned policy (permanent)

macOS official runtime assets are Apple Silicon only. The project does not
have an Apple Developer ID Application certificate and does not require one
for any release — RC, candidate, or final stable. This is a permanent project
decision that applies to the entire 1.x cycle and beyond.

When Apple Developer ID secrets are not configured in the release workflow
(the permanent default), the macOS aarch64 runtime is:

- **Always compiled.** The build step runs unconditionally; it is never
  skipped due to missing signing secrets.
- **Uploaded as unsigned.** The binary is not codesigned or notarized.
  A sidecar metadata file (`*.unsigned.json`) is uploaded alongside the
  binary so the release index and release notes can accurately reflect the
  unsigned status without modifying the frozen release-index schema.
- **Included in the channel index.** The signed index contains the macOS
  runtime artifact, its size, and SHA-256. The separate
  `*.unsigned.json` release asset records `codeSigning: "unsigned"` and
  `notarization: false`; these fields are intentionally not added to the
  frozen release-index schema.
- **Not a release blocker.** The absence of Apple Developer ID is never a
  blocking condition for any release.

macOS users may need to authorize the unsigned binary through standard
macOS controls:

1. **Finder method:** Right-click the binary in Finder and select "Open".
   A confirmation dialog appears; click "Open" again to confirm.
2. **System Settings method:** After the first launch attempt fails, open
   "System Settings → Privacy & Security" and click "Allow Anyway" next to
   the blocked binary notice.
3. **Quarantine removal (optional):** Advanced users can remove the
   quarantine extended attribute manually:
   ```bash
   xattr -d com.apple.quarantine /path/to/rill-runtime
   ```

Optional codesign remains supported: if Apple Developer ID secrets are
configured in the repository, the release workflow performs codesign.
Notarization is not currently automated. The presence or absence of
signing secrets does not change the build, upload, or index inclusion
behaviour.

## Deprecation policy

- A deprecated Stable API remains available for at least one minor 1.x
  release before removal.
- Removals happen only in a new minor release, never a patch.
- Deprecations are recorded in `CHANGELOG.md`.

## 1.x breaking-change policy

Breaking changes to Stable artifacts require a new major version (2.0):
- Rust public API
- serde model state schema (a new `format_version` is not breaking if a
  migration path exists, but removing support for an old version is)
- IPC v1/v2 wire schema
- WIT ABI v1
- model/handler pack format
- CLI subcommands and argument names

Additive changes (new Stable APIs, new error codes, new config fields via
`#[non_exhaustive]`) are allowed within 1.x.

## Release channels

| Channel | Index file | Pointer tag | Purpose |
|---|---|---|---|
| stable | `stable-index.json` | `local-ai-stable` | Final 1.0.0 and later |
| candidate | `candidate-index.json` | `local-ai-candidate` | 1.0.0-rc.x prereleases |

A candidate release never updates the stable pointer. A final release advances
the stable pointer only after its immutable public assets pass the independent
released-asset host smoke.

## Prerelease versioning

Prerelease versions follow SemVer `1.0.0-rc.N`. The version tooling
(`scripts/sync_version.py`, `scripts/release_version.py`,
`scripts/release_version_compare.py`) accepts prerelease identifiers
via strict SemVer 2.0 regex patterns. `build-release-index.py` routes
the signed index to the candidate channel via `--channel candidate`.
The release workflow (`pipeline.yml`) detects prerelease versions
(contains a `-` after the patch number), passes `--prerelease` to
`gh release create`, uploads `candidate-index.json` to the
`local-ai-candidate` pointer release, and never touches
`local-ai-stable`. The Python crate is in the Preview version group and is
published to PyPI only when its independent Preview version changes.
