//! Stable, versioned contracts shared by Rill Runtime and its hosts.

use serde::{Deserialize, Serialize};

/// IPC API version supported by this crate.
pub const RUNTIME_API_VERSION: u32 = 1;
/// Signed model-pack container version.
pub const MODEL_PACK_FORMAT_VERSION: u32 = 1;
/// Persisted host/runtime state envelope version.
pub const RUNTIME_STATE_FORMAT_VERSION: u32 = 1;
/// Signed release-index schema understood by independent updaters.
pub const RELEASE_INDEX_SCHEMA_VERSION: u32 = 1;
/// Hard upper bound for one newline-delimited IPC message.
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

pub const RUNTIME_ARTIFACT_ID: &str = "rill-runtime";

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
        if self.capabilities.is_empty() || self.capabilities.len() > 32 {
            return Err("invalid model-pack capabilities");
        }
        if self
            .capabilities
            .iter()
            .any(|capability| capability.is_empty() || capability.len() > 96)
        {
            return Err("invalid model-pack capability");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum ReleaseArtifactKind {
    Runtime,
    Model,
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
        if self.runtime_api_version != RUNTIME_API_VERSION {
            return Err("unsupported artifact runtime API version");
        }
        if self.url.is_empty() || self.url.len() > 2048 {
            return Err("invalid artifact URL");
        }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("invalid artifact SHA-256");
        }
        if self.size == 0 || self.size > 64 * 1024 * 1024 {
            return Err("invalid artifact size");
        }
        match self.kind {
            ReleaseArtifactKind::Runtime => {
                if self.id != RUNTIME_ARTIFACT_ID
                    || self.target_os.as_deref().is_none_or(str::is_empty)
                    || self.target_arch.as_deref().is_none_or(str::is_empty)
                {
                    return Err("runtime artifact requires a target OS and architecture");
                }
            }
            ReleaseArtifactKind::Model => {
                if self.target_os.is_some() || self.target_arch.is_some() {
                    return Err("model artifact must be platform independent");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_roundtrip_is_tagged_and_strict() {
        let request = RuntimeRequest::Health {
            request_id: "health-1".into(),
            api_version: RUNTIME_API_VERSION,
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
    fn handshake_fixture_is_stable() {
        let request = RuntimeRequest::Handshake {
            request_id: "fixture".into(),
            api_version: RUNTIME_API_VERSION,
            client_name: "example-host".into(),
            client_version: "0.6.10".into(),
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"method":"handshake","requestId":"fixture","apiVersion":1,"clientName":"example-host","clientVersion":"0.6.10"}"#
        );
    }

    #[test]
    fn invoke_roundtrip_preserves_capability_and_input() {
        let request = RuntimeRequest::Invoke {
            request_id: "invoke-1".into(),
            api_version: RUNTIME_API_VERSION,
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
            version: "0.5.0".into(),
            runtime_api_version: RUNTIME_API_VERSION,
            target_os: Some("macos".into()),
            target_arch: Some("aarch64".into()),
            url: "https://example.invalid/rill-runtime".into(),
            sha256: "ab".repeat(32),
            size: 1024,
        };
        assert!(runtime.validate_shape().is_ok());
        let mut model = runtime;
        model.kind = ReleaseArtifactKind::Model;
        model.id = "rillml.example.default".into();
        assert!(model.validate_shape().is_err());
        model.target_os = None;
        model.target_arch = None;
        assert!(model.validate_shape().is_ok());
    }
}
