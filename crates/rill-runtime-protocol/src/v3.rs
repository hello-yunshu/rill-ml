//! Preview Runtime IPC v3.
//!
//! V3 deliberately uses an envelope and payload types that are independent
//! from the frozen v1/v2 wire schemas. Hosts must opt in by sending
//! [`crate::v3::RUNTIME_API_VERSION_V3`]; the legacy [`crate::RUNTIME_API_VERSION`]
//! remains `2` so existing model manifests and clients retain their exact
//! meaning.

use serde::{Deserialize, Serialize};

/// Runtime IPC version used by this module.
pub const RUNTIME_API_VERSION_V3: u32 = 3;
/// Maximum request id length.
pub const MAX_REQUEST_ID_LEN_V3: usize = 128;
/// Maximum identity name length.
pub const MAX_IDENTITY_NAME_LEN_V3: usize = 96;
/// Maximum identity version length.
pub const MAX_IDENTITY_VERSION_LEN_V3: usize = 48;
/// Maximum capability length.
pub const MAX_CAPABILITY_LEN_V3: usize = 96;
/// Maximum decision id length.
pub const MAX_DECISION_ID_LEN_V3: usize = 128;
/// Maximum opaque consumer partition key length.
pub const MAX_PARTITION_KEY_LEN_V3: usize = 96;
/// Maximum feature-schema hash length (lower-case SHA-256 hex).
pub const FEATURE_SCHEMA_HASH_LEN_V3: usize = 64;
/// Maximum number of capabilities carried in a response.
pub const MAX_CAPABILITIES_V3: usize = 32;
/// Maximum error message length.
pub const MAX_ERROR_MESSAGE_LEN_V3: usize = 512;

/// Stable machine-readable marker for the opt-in v3 executable channel.
pub const PREVIEW_CHANNEL_V3: &str = "preview";

/// Bounded runtime policy shared by preview and stable v3 consumers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceProfileV1 {
    pub max_partitions: u32,
    pub max_partition_state_bytes: u32,
    pub max_total_state_bytes: u32,
    pub max_ipc_frame_bytes: u32,
    pub max_model_state_bytes: u32,
    pub max_snapshot_bytes: u32,
    pub max_handler_package_bytes: u32,
    pub max_model_pack_bytes: u32,
    pub max_features: u32,
    pub max_pending_decisions: u32,
    pub max_completed_decisions: u32,
    pub max_diagnostic_records: u32,
    pub request_deadline_ms: u64,
    pub shutdown_deadline_ms: u64,
    pub snapshot_deadline_ms: u64,
    pub restart_backoff_ms: u64,
}

impl Default for ResourceProfileV1 {
    fn default() -> Self {
        Self {
            max_partitions: 64,
            max_partition_state_bytes: 256 * 1024,
            max_total_state_bytes: 16 * 1024 * 1024,
            max_ipc_frame_bytes: crate::MAX_MESSAGE_BYTES as u32,
            max_model_state_bytes: 256 * 1024,
            max_snapshot_bytes: 512 * 1024,
            max_handler_package_bytes: 4 * 1024 * 1024,
            max_model_pack_bytes: 128 * 1024 * 1024,
            max_features: 100_000,
            max_pending_decisions: 1_024,
            max_completed_decisions: 4_096,
            max_diagnostic_records: 256,
            request_deadline_ms: 5_000,
            shutdown_deadline_ms: 2_000,
            snapshot_deadline_ms: 2_000,
            restart_backoff_ms: 100,
        }
    }
}

impl ResourceProfileV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.max_partitions == 0
            || self.max_partition_state_bytes == 0
            || self.max_total_state_bytes == 0
            || self.max_ipc_frame_bytes == 0
            || self.max_ipc_frame_bytes as usize > crate::MAX_MESSAGE_BYTES
            || self.max_model_state_bytes == 0
            || self.max_snapshot_bytes == 0
            || self.max_handler_package_bytes == 0
            || self.max_model_pack_bytes == 0
            || self.max_features == 0
            || self.max_pending_decisions == 0
            || self.max_completed_decisions == 0
            || self.max_diagnostic_records == 0
            || self.request_deadline_ms == 0
            || self.shutdown_deadline_ms == 0
            || self.snapshot_deadline_ms == 0
        {
            return Err("resource profile contains a zero limit");
        }
        if self.max_completed_decisions < self.max_pending_decisions {
            return Err("completed decision history must hold pending capacity");
        }
        Ok(())
    }
}

/// Client or runtime identity carried explicitly by V3.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityV3 {
    pub name: String,
    pub version: String,
}

impl IdentityV3 {
    /// Validate bounded identity fields.
    pub fn validate(&self) -> Result<(), ProtocolV3Error> {
        if self.name.is_empty() || self.name.len() > MAX_IDENTITY_NAME_LEN_V3 {
            return Err(ProtocolV3Error::InvalidClientIdentity);
        }
        if self.version.is_empty() || self.version.len() > MAX_IDENTITY_VERSION_LEN_V3 {
            return Err(ProtocolV3Error::InvalidClientIdentity);
        }
        Ok(())
    }
}

/// V3 request envelope. Every stateful call carries the generations and
/// feature schema against which the caller made its decision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvelopeV3 {
    pub request_id: String,
    pub api_version: u32,
    pub client_identity: IdentityV3,
    /// Opaque consumer-owned partition. Stateful requests must carry it;
    /// control requests may omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partition_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_unix_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature_schema_hash: Option<String>,
    pub model_generation: u64,
    pub state_generation: u64,
    pub payload_limit: u32,
    pub request: RuntimeRequestV3,
}

impl EnvelopeV3 {
    /// Validate shape, bounded strings, generation requirements and encoded
    /// message size. Deadline expiry is checked by the runtime because the
    /// protocol crate does not read a clock.
    pub fn validate(&self) -> Result<(), ProtocolV3Error> {
        if self.request_id.is_empty() || self.request_id.len() > MAX_REQUEST_ID_LEN_V3 {
            return Err(ProtocolV3Error::InvalidRequestId);
        }
        if self.api_version != RUNTIME_API_VERSION_V3 {
            return Err(ProtocolV3Error::IncompatibleApiVersion);
        }
        self.client_identity.validate()?;
        if self.payload_limit == 0 || self.payload_limit as usize > crate::MAX_MESSAGE_BYTES {
            return Err(ProtocolV3Error::InvalidPayloadLimit);
        }
        let is_control = matches!(
            self.request,
            RuntimeRequestV3::Handshake {} | RuntimeRequestV3::Health {}
        );
        if is_control {
            if self.capability.is_some() {
                return Err(ProtocolV3Error::UnexpectedCapability);
            }
        } else {
            let partition_key = self
                .partition_key
                .as_deref()
                .ok_or(ProtocolV3Error::InvalidPartitionKey)?;
            if partition_key.is_empty() || partition_key.len() > MAX_PARTITION_KEY_LEN_V3 {
                return Err(ProtocolV3Error::InvalidPartitionKey);
            }
            let capability = self
                .capability
                .as_deref()
                .ok_or(ProtocolV3Error::MissingCapability)?;
            if capability.is_empty() || capability.len() > MAX_CAPABILITY_LEN_V3 {
                return Err(ProtocolV3Error::InvalidCapability);
            }
            validate_schema_hash(
                self.feature_schema_hash
                    .as_deref()
                    .ok_or(ProtocolV3Error::MissingFeatureSchemaHash)?,
            )?;
        }
        if let RuntimeRequestV3::Feedback {
            decision_id,
            reward,
            ..
        } = &self.request
        {
            if decision_id.is_empty() || decision_id.len() > MAX_DECISION_ID_LEN_V3 {
                return Err(ProtocolV3Error::InvalidDecisionId);
            }
            if !reward.is_finite() {
                return Err(ProtocolV3Error::NonFiniteReward);
            }
        }
        let encoded = serde_json::to_vec(self).map_err(|_| ProtocolV3Error::InvalidJson)?;
        if encoded.len() > crate::MAX_MESSAGE_BYTES || encoded.len() > self.payload_limit as usize {
            return Err(ProtocolV3Error::PayloadTooLarge);
        }
        Ok(())
    }

    /// Whether the request deadline has elapsed at a caller-provided time.
    pub fn is_expired_at(&self, now_unix_ms: u64) -> bool {
        self.deadline_unix_ms
            .is_some_and(|deadline| now_unix_ms > deadline)
    }
}

/// Independent V3 method set. Payloads remain business-neutral JSON.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "method",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RuntimeRequestV3 {
    Handshake {},
    Health {},
    Observe {
        event: serde_json::Value,
    },
    Decide {
        context: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deterministic_seed: Option<u64>,
    },
    Feedback {
        decision_id: String,
        selected_action_id: String,
        reward: f64,
        outcome_time_ms: u64,
        generation: u64,
    },
    Inspect {},
    Snapshot {},
    Reset {
        expected_state_generation: u64,
    },
}

/// V3 response envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeResponseV3 {
    pub request_id: String,
    pub api_version: u32,
    pub runtime_identity: IdentityV3,
    pub model_generation: u64,
    pub state_generation: u64,
    pub response: RuntimeResponseBodyV3,
}

impl RuntimeResponseV3 {
    /// Validate all bounded response fields and the encoded size.
    pub fn validate(&self) -> Result<(), ProtocolV3Error> {
        if self.request_id.is_empty() || self.request_id.len() > MAX_REQUEST_ID_LEN_V3 {
            return Err(ProtocolV3Error::InvalidRequestId);
        }
        if self.api_version != RUNTIME_API_VERSION_V3 {
            return Err(ProtocolV3Error::IncompatibleApiVersion);
        }
        self.runtime_identity.validate()?;
        match &self.response {
            RuntimeResponseBodyV3::Handshake { capabilities, .. } => {
                validate_capabilities(capabilities)?;
            }
            RuntimeResponseBodyV3::Error { error } => error.validate()?,
            _ => {}
        }
        let encoded = serde_json::to_vec(self).map_err(|_| ProtocolV3Error::InvalidJson)?;
        if encoded.len() > crate::MAX_MESSAGE_BYTES {
            return Err(ProtocolV3Error::PayloadTooLarge);
        }
        Ok(())
    }
}

/// V3 response payloads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RuntimeResponseBodyV3 {
    Handshake {
        capabilities: Vec<String>,
        feature_schema_hash: String,
        handler_api_version: u32,
    },
    Health {
        healthy: bool,
    },
    Result {
        output: serde_json::Value,
    },
    Inspection {
        summary: serde_json::Value,
    },
    Snapshot {
        state_schema_version: u32,
        state_checksum: String,
        state: String,
    },
    Reset {
        reset: bool,
    },
    Error {
        error: RuntimeErrorV3,
    },
}

/// Additive response surface for the opt-in Preview subprocess. The original
/// `RuntimeResponseV3` remains frozen; this type carries channel, health and
/// decision metadata without changing its public enum variants.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeResponseV3Preview {
    pub request_id: String,
    pub api_version: u32,
    pub runtime_identity: IdentityV3,
    pub model_generation: u64,
    pub state_generation: u64,
    pub response: RuntimeResponseBodyV3Preview,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RuntimeResponseBodyV3Preview {
    Handshake {
        capabilities: Vec<String>,
        feature_schema_hash: String,
        handler_api_version: u32,
        channel: String,
    },
    Health {
        healthy: bool,
        status: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        reason_codes: Vec<String>,
    },
    Result {
        output: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decision_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decision_generation: Option<u64>,
    },
    Inspection {
        summary: serde_json::Value,
    },
    Snapshot {
        state_schema_version: u32,
        state_checksum: String,
        state: String,
    },
    Reset {
        reset: bool,
    },
    Error {
        error: RuntimeErrorV3Preview,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeErrorV3Preview {
    pub code: PreviewErrorCodeV3,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PreviewErrorCodeV3 {
    InvalidJson,
    InvalidEnvelope,
    PayloadTooLarge,
    UnsupportedCapability,
    StateMismatch,
    ExpiredRequest,
    IncompatibleGeneration,
    DuplicateDecision,
    DuplicateFeedback,
    UnknownDecision,
    StaleFeedback,
    CapacityExceeded,
    HandlerTimeout,
    HandlerTrap,
    HandlerOutputTooLarge,
    HandlerInvalidOutput,
    InvalidState,
    Internal,
}

impl PreviewErrorCodeV3 {
    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::StateMismatch
                | Self::ExpiredRequest
                | Self::HandlerTimeout
                | Self::CapacityExceeded
                | Self::Internal
        )
    }
}

/// V3 error object. Code semantics are versioned with V3 and do not alter the
/// frozen v1/v2 error-code allowlist.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeErrorV3 {
    pub code: RuntimeErrorCodeV3,
    pub message: String,
    pub retryable: bool,
}

impl RuntimeErrorV3 {
    /// Construct an error with the canonical retryability for its code.
    pub fn new(code: RuntimeErrorCodeV3, message: impl Into<String>) -> Self {
        Self {
            retryable: code.is_retryable(),
            code,
            message: message.into(),
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolV3Error> {
        if self.message.is_empty() || self.message.len() > MAX_ERROR_MESSAGE_LEN_V3 {
            return Err(ProtocolV3Error::InvalidErrorMessage);
        }
        if self.retryable != self.code.is_retryable() {
            return Err(ProtocolV3Error::InvalidRetryability);
        }
        Ok(())
    }
}

/// Exhaustive V3 error code set.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeErrorCodeV3 {
    InvalidJson,
    InvalidRequestId,
    InvalidClientIdentity,
    IncompatibleApiVersion,
    InvalidEnvelope,
    PayloadTooLarge,
    UnsupportedCapability,
    StateMismatch,
    ExpiredRequest,
    IncompatibleGeneration,
    DuplicateFeedback,
    HandlerTimeout,
    HandlerTrap,
    HandlerOutputTooLarge,
    HandlerInvalidOutput,
    InvalidState,
    Internal,
}

impl RuntimeErrorCodeV3 {
    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::StateMismatch | Self::HandlerTimeout | Self::Internal
        )
    }
}

/// Shape validation failures before runtime execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProtocolV3Error {
    InvalidJson,
    InvalidRequestId,
    InvalidPartitionKey,
    InvalidClientIdentity,
    IncompatibleApiVersion,
    InvalidPayloadLimit,
    PayloadTooLarge,
    MissingCapability,
    UnexpectedCapability,
    InvalidCapability,
    MissingFeatureSchemaHash,
    InvalidFeatureSchemaHash,
    InvalidDecisionId,
    NonFiniteReward,
    InvalidCapabilities,
    InvalidErrorMessage,
    InvalidRetryability,
}

impl std::fmt::Display for ProtocolV3Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::InvalidJson => "invalid JSON",
                Self::InvalidRequestId => "invalid request id",
                Self::InvalidPartitionKey => "invalid partition key",
                Self::InvalidClientIdentity => "invalid client identity",
                Self::IncompatibleApiVersion => "incompatible API version",
                Self::InvalidPayloadLimit => "invalid payload limit",
                Self::PayloadTooLarge => "payload too large",
                Self::MissingCapability => "missing capability",
                Self::UnexpectedCapability => "unexpected capability",
                Self::InvalidCapability => "invalid capability",
                Self::MissingFeatureSchemaHash => "missing feature schema hash",
                Self::InvalidFeatureSchemaHash => "invalid feature schema hash",
                Self::InvalidDecisionId => "invalid decision id",
                Self::NonFiniteReward => "reward must be finite",
                Self::InvalidCapabilities => "invalid capabilities",
                Self::InvalidErrorMessage => "invalid error message",
                Self::InvalidRetryability => "retryable flag does not match error code",
            }
        )
    }
}

impl std::error::Error for ProtocolV3Error {}

fn validate_schema_hash(hash: &str) -> Result<(), ProtocolV3Error> {
    if hash.len() != FEATURE_SCHEMA_HASH_LEN_V3
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ProtocolV3Error::InvalidFeatureSchemaHash);
    }
    Ok(())
}

fn validate_capabilities(capabilities: &[String]) -> Result<(), ProtocolV3Error> {
    if capabilities.is_empty() || capabilities.len() > MAX_CAPABILITIES_V3 {
        return Err(ProtocolV3Error::InvalidCapabilities);
    }
    let mut seen = std::collections::BTreeSet::new();
    if capabilities.iter().any(|capability| {
        capability.is_empty()
            || capability.len() > MAX_CAPABILITY_LEN_V3
            || !seen.insert(capability)
    }) {
        return Err(ProtocolV3Error::InvalidCapabilities);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decide() -> EnvelopeV3 {
        EnvelopeV3 {
            request_id: "decision-1".into(),
            api_version: RUNTIME_API_VERSION_V3,
            client_identity: IdentityV3 {
                name: "example-host".into(),
                version: "1.0.0".into(),
            },
            partition_key: Some("default".into()),
            capability: Some("org.example.route.decide".into()),
            deadline_unix_ms: Some(10_000),
            feature_schema_hash: Some("ab".repeat(32)),
            model_generation: 7,
            state_generation: 9,
            payload_limit: crate::MAX_MESSAGE_BYTES as u32,
            request: RuntimeRequestV3::Decide {
                context: serde_json::json!({"actions": [{"id": "route-a", "features": [1.0, 2.0]}]}),
                deterministic_seed: Some(42),
            },
        }
    }

    #[test]
    fn decide_envelope_roundtrips_and_validates() {
        let envelope = decide();
        envelope.validate().unwrap();
        let json = serde_json::to_string(&envelope).unwrap();
        let restored: EnvelopeV3 = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, envelope);
    }

    #[test]
    fn v3_rejects_unknown_fields() {
        let mut value = serde_json::to_value(decide()).unwrap();
        value["unknown"] = serde_json::json!(true);
        assert!(serde_json::from_value::<EnvelopeV3>(value).is_err());
    }

    #[test]
    fn v3_rejects_bad_hash_and_expired_deadline() {
        let mut envelope = decide();
        envelope.feature_schema_hash = Some("ABC".into());
        assert_eq!(
            envelope.validate(),
            Err(ProtocolV3Error::InvalidFeatureSchemaHash)
        );
        envelope.feature_schema_hash = Some("ab".repeat(32));
        assert!(!envelope.is_expired_at(10_000));
        assert!(envelope.is_expired_at(10_001));
    }

    #[test]
    fn v3_rejects_payload_over_declared_limit() {
        let mut envelope = decide();
        envelope.payload_limit = 128;
        assert_eq!(envelope.validate(), Err(ProtocolV3Error::PayloadTooLarge));
    }

    #[test]
    fn v3_error_retryability_is_canonical() {
        assert!(RuntimeErrorV3::new(RuntimeErrorCodeV3::HandlerTimeout, "timeout").retryable);
        assert!(!RuntimeErrorV3::new(RuntimeErrorCodeV3::DuplicateFeedback, "duplicate").retryable);
        let invalid = RuntimeErrorV3 {
            code: RuntimeErrorCodeV3::HandlerTimeout,
            message: "timeout".into(),
            retryable: false,
        };
        assert_eq!(
            invalid.validate(),
            Err(ProtocolV3Error::InvalidRetryability)
        );
    }
}
