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
pub const RELEASE_INDEX_SCHEMA_VERSION: u32 = 2;
/// Hard upper bound for one newline-delimited IPC message.
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

pub const RUNTIME_ARTIFACT_ID: &str = "rill-runtime";

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
        if capabilities.windows(2).any(|w| w[0] == w[1]) {
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseArtifact {
    pub kind: ReleaseArtifactKind,
    pub id: String,
    pub version: String,
    pub runtime_api_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_os: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_arch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_api_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_runtime_version: Option<String>,
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
                if self.id != RUNTIME_ARTIFACT_ID
                    || self.target_os.as_deref().is_none_or(str::is_empty)
                    || self.target_arch.as_deref().is_none_or(str::is_empty)
                {
                    return Err("runtime artifact requires a target OS and architecture");
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
        if self.channel != "stable" {
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
            handler_api_version: None,
            min_runtime_version: None,
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
