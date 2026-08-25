//! Stable, versioned contracts shared by Rill Runtime and its hosts.
//!
//! ## IPC API versions
//!
//! | Version | Introduced in | Changes |
//! |---|---|---|
//! | 1 | 0.5.0 | Original handshake, health, invoke |
//! | 2 | 0.7.0 | Handshake response gains handler identity and effective capabilities |
//!
//! The runtime accepts both v1 and v2 requests. v1 clients receive
//! [`RuntimeResponse`] (no handler fields). v2 clients receive
//! [`RuntimeResponseV2`] (with handler identity). The two wire schemas are
//! independently frozen with fixture tests.

use serde::{Deserialize, Serialize};

/// Preview IPC v3 types. This module is independent from the frozen v1/v2
/// request and response types below.
pub mod v3;

/// Minimum IPC API version the runtime still accepts.
pub const MIN_RUNTIME_API_VERSION: u32 = 1;
/// Latest IPC API version supported by this crate.
pub const RUNTIME_API_VERSION: u32 = 2;
/// Signed model-pack container version.
pub const MODEL_PACK_FORMAT_VERSION: u32 = 1;
/// Signed handler-pack container version.
pub const HANDLER_PACKAGE_FORMAT_VERSION: u32 = 1;
/// Handler ABI version (independent of IPC API version).
pub const HANDLER_API_VERSION: u32 = 1;
/// Persisted host/runtime state envelope version.
pub const RUNTIME_STATE_FORMAT_VERSION: u32 = 1;
/// Signed release-index schema understood by independent updaters.
///
/// v3 is the current frozen stable schema. It adds an explicit ``target_libc``
/// field (``gnu``/``musl``) to Linux runtime artifacts so libc variants of the
/// same OS+arch are disambiguated deterministically. v3 is a versioned schema:
/// a v1.1.0 reader (whose validator requires ``RELEASE_INDEX_SCHEMA_VERSION
/// == 2``) rejects it at the schema boundary (fail-closed) rather than
/// naive-matching gnu and musl builds to the same OS+arch and failing
/// ambiguously.
pub const RELEASE_INDEX_SCHEMA_VERSION: u32 = 3;
/// Version of the external trust metadata used for key rotation and rollback
/// protection. This is deliberately separate from the frozen release-index v3
/// payload, so existing v1.2 readers keep their exact fail-closed semantics.
pub const TRUST_METADATA_SCHEMA_VERSION: u32 = 1;
/// Version of the signed release-generation envelope used with trust metadata.
pub const RELEASE_INDEX_LIFECYCLE_SCHEMA_VERSION: u32 = 1;
/// Hard upper bound for one newline-delimited IPC message.
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// Stable artifact id for the GNU (default) runtime build.
pub const RUNTIME_ARTIFACT_ID: &str = "rill-runtime";
/// Stable artifact id for the musl runtime build. The libc variant is part of
/// the stable asset identity so gnu and musl builds of the same OS+arch do not
/// collide in a v2 release index.
pub const RUNTIME_ARTIFACT_ID_MUSL: &str = "rill-runtime-musl";
/// Historical artifact id retained so v1.5.1 release indexes remain readable.
/// RillML v1.5.2 does not build or publish this adapter.
pub const PM_ADAPTER_ARTIFACT_ID: &str = "rill-pm-adapter";
/// Historical ``pm-rill-shadow`` protocol version in legacy PM adapter indexes.
pub const PM_ADAPTER_PROTOCOL_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Stable IPC error codes
// ---------------------------------------------------------------------------

/// Stable IPC error code constants.
///
/// Every `RuntimeResponse::Error` / `RuntimeResponseV2::Error` `code` field
/// produced by the runtime is one of the constants in this module. The codes
/// are frozen for the entire 1.x cycle: existing codes are never renamed, and
/// new codes may only be added (additive).
///
/// The runtime constructs error responses exclusively from these constants.
/// Hosts and clients may switch on the string values; the constants are
/// exported so that downstream Rust code does not have to inline string
/// literals.
pub mod error_code {
    /// Request body was not valid protocol JSON.
    pub const INVALID_JSON: &str = "invalidJson";
    /// `requestId` was missing, empty, or longer than 128 characters.
    pub const INVALID_REQUEST_ID: &str = "invalidRequestId";
    /// `apiVersion` was outside `[MIN_RUNTIME_API_VERSION, RUNTIME_API_VERSION]`.
    pub const INCOMPATIBLE_API_VERSION: &str = "incompatibleApiVersion";
    /// `clientName` / `clientVersion` failed length or emptiness checks.
    pub const INVALID_CLIENT_IDENTITY: &str = "invalidClientIdentity";
    /// `Invoke` capability is not in the effective capability set.
    pub const UNSUPPORTED_CAPABILITY: &str = "unsupportedCapability";
    /// `Invoke` was issued but no handler is registered.
    pub const NO_INVOKE_HANDLER: &str = "noInvokeHandler";
    /// Handler exceeded the wall-clock deadline. Retryable.
    pub const HANDLER_TIMEOUT: &str = "handlerTimeout";
    /// Handler trapped (unreachable, out-of-bounds, stack overflow, …).
    pub const HANDLER_TRAP: &str = "handlerTrap";
    /// Handler output exceeded the host-side size limit.
    pub const HANDLER_OUTPUT_TOO_LARGE: &str = "handlerOutputTooLarge";
    /// Handler output was not valid JSON.
    pub const HANDLER_INVALID_OUTPUT: &str = "handlerInvalidOutput";
    /// Handler reported an internal error (covers all four WIT
    /// `handler-error` variants on the wire for backwards compatibility).
    pub const HANDLER_INTERNAL_ERROR: &str = "handlerInternalError";

    /// All frozen error codes in alphabetical order.
    ///
    /// This slice is used by tests and by the runtime's error-code allowlist
    /// check. Adding a new code requires appending to this slice; the order
    /// is part of the frozen surface so test fixtures remain stable.
    pub const FROZEN_CODES: &[&str] = &[
        HANDLER_INTERNAL_ERROR,
        HANDLER_INVALID_OUTPUT,
        HANDLER_OUTPUT_TOO_LARGE,
        HANDLER_TIMEOUT,
        HANDLER_TRAP,
        INCOMPATIBLE_API_VERSION,
        INVALID_CLIENT_IDENTITY,
        INVALID_JSON,
        INVALID_REQUEST_ID,
        NO_INVOKE_HANDLER,
        UNSUPPORTED_CAPABILITY,
    ];

    /// Returns `true` if `code` is one of the frozen 1.x error codes.
    pub fn is_frozen(code: &str) -> bool {
        FROZEN_CODES.contains(&code)
    }
}

// ---------------------------------------------------------------------------
// Model pack manifest
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelPackManifest {
    pub format_version: u32,
    pub id: String,
    pub version: String,
    pub runtime_api_version: u32,
    pub min_runtime_version: String,
    pub publisher_key_id: String,
    pub capabilities: Vec<String>,
}

impl ModelPackManifest {
    pub fn validate_shape(&self) -> Result<(), &'static str> {
        if self.format_version != MODEL_PACK_FORMAT_VERSION {
            return Err("unsupported model-pack format version");
        }
        if self.runtime_api_version != RUNTIME_API_VERSION {
            return Err("unsupported runtime API version");
        }
        if self.id.is_empty() || self.id.len() > 96 {
            return Err("invalid model-pack id");
        }
        if self.version.is_empty() || self.version.len() > 48 {
            return Err("invalid model-pack version");
        }
        if self.publisher_key_id.is_empty() || self.publisher_key_id.len() > 96 {
            return Err("invalid publisher key id");
        }
        Self::validate_capabilities(&self.capabilities)?;
        Ok(())
    }

    pub fn validate_capabilities(capabilities: &[String]) -> Result<(), &'static str> {
        if capabilities.is_empty() || capabilities.len() > 32 {
            return Err("invalid capabilities list");
        }
        if capabilities
            .iter()
            .any(|capability| capability.is_empty() || capability.len() > 96)
        {
            return Err("invalid capability string");
        }
        let mut seen = std::collections::HashSet::new();
        if !capabilities
            .iter()
            .all(|capability| seen.insert(capability.clone()))
        {
            return Err("duplicate capability");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Handler pack manifest
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HandlerPackManifest {
    pub format_version: u32,
    pub id: String,
    pub version: String,
    pub handler_api_version: u32,
    pub min_runtime_version: String,
    pub publisher_key_id: String,
    pub capabilities: Vec<String>,
    pub module_sha256: String,
    pub module_size: u64,
}

impl HandlerPackManifest {
    pub fn validate_shape(&self) -> Result<(), &'static str> {
        if self.format_version != HANDLER_PACKAGE_FORMAT_VERSION {
            return Err("unsupported handler-pack format version");
        }
        if self.handler_api_version != HANDLER_API_VERSION {
            return Err("unsupported handler API version");
        }
        if self.id.is_empty() || self.id.len() > 96 {
            return Err("invalid handler id");
        }
        if self.version.is_empty() || self.version.len() > 48 {
            return Err("invalid handler version");
        }
        if self.publisher_key_id.is_empty() || self.publisher_key_id.len() > 96 {
            return Err("invalid handler publisher key id");
        }
        if self.min_runtime_version.is_empty() || self.min_runtime_version.len() > 48 {
            return Err("invalid minimum runtime version");
        }
        ModelPackManifest::validate_capabilities(&self.capabilities)?;
        if self.module_sha256.len() != 64
            || !self
                .module_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("invalid module SHA-256");
        }
        if self.module_size == 0 || self.module_size > 4 * 1024 * 1024 {
            return Err("invalid module size");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Release index
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum ReleaseArtifactKind {
    Runtime,
    Model,
    Handler,
    #[serde(rename = "pm-adapter")]
    PmAdapter,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseArtifact {
    pub kind: ReleaseArtifactKind,
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub runtime_api_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_os: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_arch: Option<String>,
    /// The libc/ABI variant (``gnu`` or ``musl``) of a Linux target. Present
    /// on Linux runtime artifacts and historical adapter artifacts;
    /// non-Linux targets (macOS, Windows, FreeBSD) omit it. Introduced in release-index schema v3 so
    /// gnu and musl builds of the same OS+arch coexist unambiguously.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_libc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_api_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_runtime_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pm_adapter_protocol_version: Option<u32>,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

impl ReleaseArtifact {
    pub fn validate_shape(&self) -> Result<(), &'static str> {
        if self.id.is_empty() || self.id.len() > 96 {
            return Err("invalid artifact id");
        }
        if self.version.is_empty() || self.version.len() > 48 {
            return Err("invalid artifact version");
        }
        if self.url.is_empty() || self.url.len() > 2048 {
            return Err("invalid artifact URL");
        }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("invalid artifact SHA-256");
        }
        if self.size == 0 || self.size > 128 * 1024 * 1024 {
            return Err("invalid artifact size");
        }
        match self.kind {
            ReleaseArtifactKind::Runtime => {
                if self.runtime_api_version != RUNTIME_API_VERSION {
                    return Err("unsupported artifact runtime API version");
                }
                // The artifact ``id`` is part of the stable asset identity. On
                // Linux, the libc/ABI variant (gnu vs musl) is encoded in the
                // ``id`` and recorded explicitly in ``target_libc`` so both
                // builds of the same OS+arch coexist unambiguously in a v3
                // index.
                if (self.id != RUNTIME_ARTIFACT_ID && self.id != RUNTIME_ARTIFACT_ID_MUSL)
                    || self.target_os.as_deref().is_none_or(str::is_empty)
                    || self.target_arch.as_deref().is_none_or(str::is_empty)
                {
                    return Err("runtime artifact requires a target OS and architecture");
                }
                // On Linux the libc variant must be explicit (gnu or musl).
                // Non-Linux targets must not carry a libc variant.
                match self.target_os.as_deref() {
                    Some("linux") => {
                        let libc = self.target_libc.as_deref();
                        let expected = if self.id == RUNTIME_ARTIFACT_ID_MUSL {
                            Some("musl")
                        } else {
                            Some("gnu")
                        };
                        if libc != expected {
                            return Err("runtime artifact libc variant does not match its id");
                        }
                    }
                    _ => {
                        if self.target_libc.is_some() {
                            return Err("non-Linux runtime artifact must not carry a libc variant");
                        }
                    }
                }
                if self.handler_api_version.is_some() || self.min_runtime_version.is_some() {
                    return Err("runtime artifact must not carry handler fields");
                }
            }
            ReleaseArtifactKind::Model => {
                if self.runtime_api_version != RUNTIME_API_VERSION {
                    return Err("unsupported artifact runtime API version");
                }
                if self.target_os.is_some()
                    || self.target_arch.is_some()
                    || self.handler_api_version.is_some()
                    || self.min_runtime_version.is_some()
                {
                    return Err("model artifact must be platform independent");
                }
            }
            ReleaseArtifactKind::Handler => {
                if self.runtime_api_version != RUNTIME_API_VERSION {
                    return Err("unsupported artifact runtime API version");
                }
                if self.target_os.is_some() || self.target_arch.is_some() {
                    return Err("handler artifact must be platform independent");
                }
                let handler_api = self
                    .handler_api_version
                    .ok_or("handler artifact requires handler API version")?;
                if handler_api != HANDLER_API_VERSION {
                    return Err("unsupported handler API version");
                }
                let min_runtime = self
                    .min_runtime_version
                    .as_deref()
                    .ok_or("handler artifact requires minimum runtime version")?;
                if min_runtime.is_empty() || min_runtime.len() > 48 {
                    return Err("invalid minimum runtime version");
                }
            }
            ReleaseArtifactKind::PmAdapter => {
                // The PM adapter speaks the independent ``pm-rill-shadow``
                // protocol, not the Rill Runtime IPC API, so
                // ``runtimeApiVersion`` is not applicable and must remain
                // unset (serde default 0).
                if self.runtime_api_version != 0 {
                    return Err("pm-adapter artifact must not set runtime API version");
                }
                if self.id != PM_ADAPTER_ARTIFACT_ID
                    || self.target_os.as_deref().is_none_or(str::is_empty)
                    || self.target_arch.as_deref().is_none_or(str::is_empty)
                {
                    return Err("pm-adapter artifact requires a target OS and architecture");
                }
                if self.handler_api_version.is_some() || self.min_runtime_version.is_some() {
                    return Err("pm-adapter artifact must not carry handler fields");
                }
                if self.pm_adapter_protocol_version != Some(PM_ADAPTER_PROTOCOL_VERSION) {
                    return Err("unsupported pm-adapter protocol version");
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseIndexPayload {
    pub schema_version: u32,
    pub channel: String,
    pub generated_at: String,
    pub publisher_key_id: String,
    pub artifacts: Vec<ReleaseArtifact>,
}

impl ReleaseIndexPayload {
    pub fn validate_shape(&self) -> Result<(), &'static str> {
        if self.schema_version != RELEASE_INDEX_SCHEMA_VERSION {
            return Err("unsupported release-index schema");
        }
        if !matches!(self.channel.as_str(), "stable" | "candidate") {
            return Err("unsupported release channel");
        }
        if self.generated_at.is_empty() || self.generated_at.len() > 64 {
            return Err("invalid release-index timestamp");
        }
        if self.publisher_key_id.is_empty() || self.publisher_key_id.len() > 96 {
            return Err("invalid release-index publisher");
        }
        if self.artifacts.is_empty() || self.artifacts.len() > 64 {
            return Err("invalid release-index artifact count");
        }
        for artifact in &self.artifacts {
            artifact.validate_shape()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignedReleaseIndex {
    pub payload: ReleaseIndexPayload,
    /// Lowercase hexadecimal Ed25519 signature over canonical payload JSON.
    pub signature: String,
}

/// A trusted publisher key and its lifecycle state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustKeyMetadataV1 {
    pub key_id: String,
    pub public_key_hex: String,
    pub role: TrustKeyRole,
    pub not_before_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_after_unix_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at_unix_ms: Option<u64>,
    #[serde(default)]
    pub emergency_revoked: bool,
}

/// Rotation role for a trusted publisher key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TrustKeyRole {
    Current,
    Next,
}

impl TrustKeyMetadataV1 {
    pub fn validate_shape(&self) -> Result<(), &'static str> {
        if self.key_id.is_empty() || self.key_id.len() > 96 {
            return Err("invalid trust key id");
        }
        if self.public_key_hex.len() != 64
            || !self
                .public_key_hex
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("invalid trust public key");
        }
        if let Some(not_after) = self.not_after_unix_ms
            && not_after <= self.not_before_unix_ms
        {
            return Err("trust key validity window is empty");
        }
        Ok(())
    }

    /// Whether this key may authenticate an artifact at `now_unix_ms`.
    pub fn is_active_at(&self, now_unix_ms: u64) -> bool {
        self.not_before_unix_ms <= now_unix_ms
            && self
                .not_after_unix_ms
                .is_none_or(|not_after| now_unix_ms < not_after)
            && self
                .revoked_at_unix_ms
                .is_none_or(|revoked_at| now_unix_ms < revoked_at)
            && !self.emergency_revoked
    }
}

/// External trust metadata for publisher key rotation.
///
/// The metadata is signed/distributed by the consumer's trust root. It is not
/// embedded into release-index schema v3. `minimumReleaseGeneration` is the
/// metadata publisher's release floor; consumers enforce their independent
/// release floor through [`TrustVerificationFloorV1`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustMetadataV1 {
    pub schema_version: u32,
    pub metadata_generation: u64,
    pub minimum_release_generation: u64,
    pub keys: Vec<TrustKeyMetadataV1>,
}

/// Monotonic state persisted by a consumer that opts into trust-metadata
/// lifecycle verification.
///
/// RillML does not persist this value on behalf of a host. A consumer should
/// atomically replace it only after accepting authenticated metadata. When a
/// digest is present, the metadata bytes at the same generation are also
/// pinned, so a same-generation content fork fails closed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustVerificationFloorV1 {
    pub minimum_metadata_generation: u64,
    pub minimum_release_generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_digest: Option<String>,
}

impl TrustMetadataV1 {
    pub fn validate_shape(&self) -> Result<(), &'static str> {
        if self.schema_version != TRUST_METADATA_SCHEMA_VERSION {
            return Err("unsupported trust metadata schema");
        }
        if self.keys.is_empty() || self.keys.len() > 64 {
            return Err("invalid trust key count");
        }
        let mut ids = std::collections::BTreeSet::new();
        for key in &self.keys {
            key.validate_shape()?;
            if !ids.insert(&key.key_id) {
                return Err("duplicate trust key id");
            }
        }
        if !self
            .keys
            .iter()
            .any(|key| matches!(&key.role, TrustKeyRole::Current))
        {
            return Err("trust metadata has no current key");
        }
        Ok(())
    }

    /// Returns the active current/next keys at a point in time.
    pub fn active_keys_at(
        &self,
        now_unix_ms: u64,
    ) -> Result<Vec<&TrustKeyMetadataV1>, &'static str> {
        self.validate_shape()?;
        let active: Vec<_> = self
            .keys
            .iter()
            .filter(|key| key.is_active_at(now_unix_ms))
            .collect();
        if !active
            .iter()
            .any(|key| matches!(&key.role, TrustKeyRole::Current))
        {
            return Err("trust metadata has no active current key");
        }
        Ok(active)
    }
}

/// A signed v3 index plus a monotonic generation used for rollback protection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignedReleaseIndexWithGenerationV1 {
    pub schema_version: u32,
    pub release_generation: u64,
    pub index: SignedReleaseIndex,
    /// Lowercase hexadecimal signature over the lifecycle envelope fields and
    /// the embedded signed index. This binds the generation to the publisher.
    pub lifecycle_signature: String,
}

impl SignedReleaseIndexWithGenerationV1 {
    pub fn validate_shape(&self) -> Result<(), &'static str> {
        if self.schema_version != RELEASE_INDEX_LIFECYCLE_SCHEMA_VERSION {
            return Err("unsupported release lifecycle schema");
        }
        if self.lifecycle_signature.len() != 128
            || !self
                .lifecycle_signature
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("invalid release lifecycle signature");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// IPC requests (shared by v1 and v2)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "method",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RuntimeRequest {
    Handshake {
        request_id: String,
        api_version: u32,
        client_name: String,
        client_version: String,
    },
    Health {
        request_id: String,
        api_version: u32,
    },
    Invoke {
        request_id: String,
        api_version: u32,
        capability: String,
        input: serde_json::Value,
    },
}

impl RuntimeRequest {
    pub fn request_id(&self) -> &str {
        match self {
            Self::Handshake { request_id, .. }
            | Self::Health { request_id, .. }
            | Self::Invoke { request_id, .. } => request_id,
        }
    }

    pub fn api_version(&self) -> u32 {
        match self {
            Self::Handshake { api_version, .. }
            | Self::Health { api_version, .. }
            | Self::Invoke { api_version, .. } => *api_version,
        }
    }
}

// ---------------------------------------------------------------------------
// IPC v1 responses (frozen since 0.5.0)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RuntimeResponse {
    Handshake {
        request_id: String,
        api_version: u32,
        runtime_version: String,
        model_pack_id: String,
        model_pack_version: String,
        capabilities: Vec<String>,
    },
    Health {
        request_id: String,
        api_version: u32,
        healthy: bool,
        model_pack_id: String,
        model_pack_version: String,
    },
    Result {
        request_id: String,
        api_version: u32,
        output: serde_json::Value,
    },
    Error {
        request_id: String,
        api_version: u32,
        code: String,
        message: String,
        retryable: bool,
    },
}

// ---------------------------------------------------------------------------
// IPC v2 responses (introduced in 0.7.0)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RuntimeResponseV2 {
    Handshake {
        request_id: String,
        api_version: u32,
        runtime_version: String,
        model_pack_id: String,
        model_pack_version: String,
        capabilities: Vec<String>,
        handler_id: String,
        handler_version: String,
        handler_api_version: u32,
        effective_capabilities: Vec<String>,
    },
    Health {
        request_id: String,
        api_version: u32,
        healthy: bool,
        model_pack_id: String,
        model_pack_version: String,
    },
    Result {
        request_id: String,
        api_version: u32,
        output: serde_json::Value,
    },
    Error {
        request_id: String,
        api_version: u32,
        code: String,
        message: String,
        retryable: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_v1_roundtrip_is_tagged_and_strict() {
        let request = RuntimeRequest::Health {
            request_id: "health-1".into(),
            api_version: 1,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"method\":\"health\""));
        let restored: RuntimeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, request);
        assert!(
            serde_json::from_str::<RuntimeRequest>(
                r#"{"method":"health","requestId":"x","apiVersion":1,"extra":true}"#
            )
            .is_err()
        );
    }

    #[test]
    fn v1_handshake_fixture_is_stable() {
        let request = RuntimeRequest::Handshake {
            request_id: "fixture".into(),
            api_version: 1,
            client_name: "example-host".into(),
            client_version: "0.6.10".into(),
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"method":"handshake","requestId":"fixture","apiVersion":1,"clientName":"example-host","clientVersion":"0.6.10"}"#
        );
    }

    #[test]
    fn v1_handshake_response_fixture_is_stable() {
        let response = RuntimeResponse::Handshake {
            request_id: "fixture".into(),
            api_version: 1,
            runtime_version: "0.6.0".into(),
            model_pack_id: "rillml.example.default".into(),
            model_pack_version: "0.6.0".into(),
            capabilities: vec!["rillml.example".into()],
        };
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            r#"{"kind":"handshake","requestId":"fixture","apiVersion":1,"runtimeVersion":"0.6.0","modelPackId":"rillml.example.default","modelPackVersion":"0.6.0","capabilities":["rillml.example"]}"#
        );
    }

    #[test]
    fn v2_handshake_response_fixture_is_stable() {
        let response = RuntimeResponseV2::Handshake {
            request_id: "v2-fixture".into(),
            api_version: 2,
            runtime_version: "0.7.0".into(),
            model_pack_id: "rillml.example.default".into(),
            model_pack_version: "0.7.0".into(),
            capabilities: vec!["rillml.example".into()],
            handler_id: "org.example.handler".into(),
            handler_version: "1.0.0".into(),
            handler_api_version: 1,
            effective_capabilities: vec!["rillml.example".into()],
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"handlerId\":\"org.example.handler\""));
        assert!(json.contains("\"handlerApiVersion\":1"));
        assert!(json.contains("\"effectiveCapabilities\":[\"rillml.example\"]"));
        // Mutating the response produces a different JSON, proving the fixture
        // is fully serialised and not relying on default values.
        let mut bad = serde_json::from_str::<RuntimeResponseV2>(&json).unwrap();
        if let RuntimeResponseV2::Handshake { handler_id, .. } = &mut bad {
            handler_id.push('x');
        }
        let bad_json = serde_json::to_string(&bad).unwrap();
        assert_ne!(bad_json, json);
    }

    #[test]
    fn v1_response_rejects_handler_fields() {
        let json = r#"{"kind":"handshake","requestId":"x","apiVersion":1,"runtimeVersion":"0.7.0","modelPackId":"m","modelPackVersion":"1","capabilities":["c"],"handlerId":"h"}"#;
        assert!(serde_json::from_str::<RuntimeResponse>(json).is_err());
    }

    #[test]
    fn invoke_roundtrip_preserves_capability_and_input() {
        let request = RuntimeRequest::Invoke {
            request_id: "invoke-1".into(),
            api_version: 2,
            capability: "rillml.example".into(),
            input: serde_json::json!({"samples": []}),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"method\":\"invoke\""));
        assert!(json.contains("\"capability\":\"rillml.example\""));
        let restored: RuntimeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, request);
    }

    #[test]
    fn release_artifacts_enforce_platform_boundaries() {
        let runtime = ReleaseArtifact {
            kind: ReleaseArtifactKind::Runtime,
            id: RUNTIME_ARTIFACT_ID.into(),
            version: "0.7.0".into(),
            runtime_api_version: RUNTIME_API_VERSION,
            target_os: Some("macos".into()),
            target_arch: Some("aarch64".into()),
            target_libc: None,
            handler_api_version: None,
            min_runtime_version: None,
            pm_adapter_protocol_version: None,
            url: "https://example.invalid/rill-runtime".into(),
            sha256: "ab".repeat(32),
            size: 1024,
        };
        assert!(runtime.validate_shape().is_ok());

        let mut model = runtime.clone();
        model.kind = ReleaseArtifactKind::Model;
        model.id = "rillml.example.default".into();
        model.target_os = None;
        model.target_arch = None;
        assert!(model.validate_shape().is_ok());

        let mut handler = runtime.clone();
        handler.kind = ReleaseArtifactKind::Handler;
        handler.id = "org.example.handler".into();
        handler.target_os = None;
        handler.target_arch = None;
        handler.handler_api_version = Some(HANDLER_API_VERSION);
        handler.min_runtime_version = Some("0.7.0".into());
        assert!(handler.validate_shape().is_ok());

        // Handler with platform fields is rejected.
        handler.target_os = Some("linux".into());
        assert!(handler.validate_shape().is_err());
        handler.target_os = None;

        // Handler without handler_api_version is rejected.
        handler.handler_api_version = None;
        assert!(handler.validate_shape().is_err());
        handler.handler_api_version = Some(HANDLER_API_VERSION);

        // Handler without min_runtime_version is rejected.
        handler.min_runtime_version = None;
        assert!(handler.validate_shape().is_err());
    }

    #[test]
    fn pm_adapter_artifact_roundtrip_and_shape_validation() {
        // The release index emits kebab-case ``pm-adapter`` (not camelCase
        // ``pmAdapter``).
        let json = r#"{"kind":"pm-adapter","id":"rill-pm-adapter","version":"1.2.0-rc.1","pmAdapterProtocolVersion":1,"targetOs":"linux","targetArch":"x86_64","url":"https://example.invalid/adapter","sha256":"3333333333333333333333333333333333333333333333333333333333333333","size":4096}"#;
        let artifact: ReleaseArtifact = serde_json::from_str(json).unwrap();
        assert_eq!(artifact.kind, ReleaseArtifactKind::PmAdapter);
        assert_eq!(
            artifact.pm_adapter_protocol_version,
            Some(PM_ADAPTER_PROTOCOL_VERSION)
        );
        assert!(artifact.validate_shape().is_ok());

        // Unknown kind is still rejected.
        assert!(
            serde_json::from_str::<ReleaseArtifact>(&json.replace("pm-adapter", "pmAdapter"))
                .is_err()
        );

        // Wrong protocol version is rejected.
        let mut bad = artifact.clone();
        bad.pm_adapter_protocol_version = Some(99);
        assert!(bad.validate_shape().is_err());

        // Handler fields are rejected on a pm-adapter.
        let mut bad = artifact.clone();
        bad.handler_api_version = Some(HANDLER_API_VERSION);
        assert!(bad.validate_shape().is_err());

        // Setting a runtime API version is rejected on a pm-adapter.
        let mut bad = artifact.clone();
        bad.runtime_api_version = RUNTIME_API_VERSION;
        assert!(bad.validate_shape().is_err());

        // Missing target platform is rejected.
        let mut bad = artifact.clone();
        bad.target_os = None;
        assert!(bad.validate_shape().is_err());

        // Wrong artifact id is rejected.
        let mut bad = artifact.clone();
        bad.id = "org.example.other".into();
        assert!(bad.validate_shape().is_err());
    }

    #[test]
    fn historical_v151_pm_adapter_index_remains_readable() {
        let fixture =
            include_str!("../../../tests/fixtures/legacy/rill-v1.5.1-pm-adapter-index.json");
        let index: SignedReleaseIndex = serde_json::from_str(fixture).unwrap();
        assert_eq!(index.payload.schema_version, RELEASE_INDEX_SCHEMA_VERSION);
        assert_eq!(index.payload.artifacts.len(), 1);
        assert_eq!(
            index.payload.artifacts[0].kind,
            ReleaseArtifactKind::PmAdapter
        );
        assert_eq!(index.payload.artifacts[0].version, "1.5.1");
        assert!(index.payload.validate_shape().is_ok());
    }

    #[test]
    fn handler_manifest_validates_shape() {
        let manifest = HandlerPackManifest {
            format_version: HANDLER_PACKAGE_FORMAT_VERSION,
            id: "org.example.handler".into(),
            version: "1.0.0".into(),
            handler_api_version: HANDLER_API_VERSION,
            min_runtime_version: "0.7.0".into(),
            publisher_key_id: "test-key".into(),
            capabilities: vec!["org.example.predict".into()],
            module_sha256: "ab".repeat(32),
            module_size: 1024,
        };
        assert!(manifest.validate_shape().is_ok());

        let mut bad = manifest.clone();
        bad.format_version = 99;
        assert!(bad.validate_shape().is_err());

        let mut bad = manifest.clone();
        bad.handler_api_version = 99;
        assert!(bad.validate_shape().is_err());

        let mut bad = manifest.clone();
        bad.capabilities = vec![];
        assert!(bad.validate_shape().is_err());

        let mut bad = manifest.clone();
        bad.module_sha256 = "short".into();
        assert!(bad.validate_shape().is_err());

        let mut bad = manifest.clone();
        bad.module_size = 0;
        assert!(bad.validate_shape().is_err());
    }
}
