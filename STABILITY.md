# RillML 1.0 Stability Policy

This document defines the compatibility surface that RillML 1.x commits to
preserve. It is the single source of truth for what is stable, what is
preview, and how each artifact may evolve.

## Stability matrix

| Artifact | 1.0 status | Stable commitment |
|---|---|---|
| `rill-ml` | Stable | Rust public API, serde model state |
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
serializable model type additionally implements `ValidateState` to enforce
type-specific invariants (dimensions, finite values, non-negative counts,
optimizer parameter counts, FTRL `max_features`, encoder mapping
consistency, pipeline dimensions, bandit arm state, drift detector buffer).

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

RC and all future 1.x CI must load every fixture. A schema bump requires a
migration note in `CHANGELOG.md` and a new fixture set.

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
| macOS aarch64 (Apple Silicon) | Stable (signed) | Stable |
| macOS x86_64 (Intel) | Not published | Stable |

macOS official runtime assets are Apple Silicon only and must be codesigned.
Unsigned macOS assets are not published as official stable or candidate
artifacts.

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

A candidate release never updates the stable pointer. The stable pointer
remains at `0.13.0` until the final `1.0.0` release.

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
`local-ai-stable`. The Python crate is in the Preview version group
and is not published to PyPI during the 1.0 RC cycle.
