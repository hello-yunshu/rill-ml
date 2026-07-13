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

pub const BATTERY_USAGE_CAPABILITY: &str = "batteryUsage";
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatteryModelConfig {
    pub feature_count: usize,
    pub learning_rate: f64,
    pub l2: f64,
    pub huber_delta: f64,
    pub min_training_samples: u64,
    pub min_validation_samples: u64,
    pub quality_window: usize,
    pub required_error_ratio: f64,
    pub max_drain_per_hour: f64,
    pub max_remaining_hours: f64,
    pub session_gap_minutes: i64,
    pub replacement_rise_percent: u8,
    pub min_drop_percent: f64,
    pub baseline_decay_tau_hours: f64,
}

impl Default for BatteryModelConfig {
    fn default() -> Self {
        Self {
            feature_count: 6,
            learning_rate: 0.03,
            l2: 0.001,
            huber_delta: 5.0,
            min_training_samples: 6,
            min_validation_samples: 8,
            quality_window: 24,
            required_error_ratio: 0.98,
            max_drain_per_hour: 50.0,
            max_remaining_hours: 9999.0,
            session_gap_minutes: 10,
            replacement_rise_percent: 5,
            min_drop_percent: 1.0,
            baseline_decay_tau_hours: 48.0,
        }
    }
}

impl BatteryModelConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        let finite = [
            self.learning_rate,
            self.l2,
            self.huber_delta,
            self.required_error_ratio,
            self.max_drain_per_hour,
            self.max_remaining_hours,
            self.min_drop_percent,
            self.baseline_decay_tau_hours,
        ]
        .into_iter()
        .all(f64::is_finite);
        if !finite {
            return Err("battery model contains non-finite parameters");
        }
        if self.feature_count != 6 {
            return Err("unsupported battery feature schema");
        }
        if self.learning_rate <= 0.0
            || self.l2 < 0.0
            || self.huber_delta <= 0.0
            || self.min_training_samples == 0
            || self.min_validation_samples == 0
            || self.quality_window < self.min_validation_samples as usize
            || !(0.0..1.0).contains(&self.required_error_ratio)
            || self.max_drain_per_hour <= 0.0
            || self.max_remaining_hours <= 0.0
            || self.session_gap_minutes <= 0
            || self.replacement_rise_percent == 0
            || self.replacement_rise_percent > 100
            || self.min_drop_percent <= 0.0
            || self.baseline_decay_tau_hours <= 0.0
        {
            return Err("invalid battery model parameters");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatterySampleInput {
    pub at_unix_ms: i64,
    pub percentage: u8,
    pub charging: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatteryPredictionInput {
    pub now_unix_ms: i64,
    pub samples: Vec<BatterySampleInput>,
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
    BatteryPredict {
        request_id: String,
        api_version: u32,
        input: BatteryPredictionInput,
    },
}

impl RuntimeRequest {
    pub fn request_id(&self) -> &str {
        match self {
            Self::Handshake { request_id, .. }
            | Self::Health { request_id, .. }
            | Self::BatteryPredict { request_id, .. } => request_id,
        }
    }

    pub fn api_version(&self) -> u32 {
        match self {
            Self::Handshake { api_version, .. }
            | Self::Health { api_version, .. }
            | Self::BatteryPredict { api_version, .. } => *api_version,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PredictionSource {
    LocalAi,
    BaselineRecommended,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatteryPredictionOutput {
    pub remaining_hours: Option<f64>,
    pub source: PredictionSource,
    pub reason: String,
    pub training_samples: u64,
    pub validation_samples: u64,
    pub baseline_mae: Option<f64>,
    pub candidate_mae: Option<f64>,
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
    BatteryPrediction {
        request_id: String,
        api_version: u32,
        output: BatteryPredictionOutput,
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
    fn mira_handshake_fixture_is_stable() {
        let request = RuntimeRequest::Handshake {
            request_id: "fixture".into(),
            api_version: RUNTIME_API_VERSION,
            client_name: "mira".into(),
            client_version: "0.6.10".into(),
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"method":"handshake","requestId":"fixture","apiVersion":1,"clientName":"mira","clientVersion":"0.6.10"}"#
        );
    }

    #[test]
    fn battery_config_rejects_unsafe_values() {
        assert!(BatteryModelConfig::default().validate().is_ok());
        assert!(
            BatteryModelConfig {
                quality_window: 0,
                ..BatteryModelConfig::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            BatteryModelConfig {
                learning_rate: f64::NAN,
                ..BatteryModelConfig::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            BatteryModelConfig {
                max_remaining_hours: 0.0,
                ..BatteryModelConfig::default()
            }
            .validate()
            .is_err()
        );
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
        model.id = "mira.battery.default".into();
        assert!(model.validate_shape().is_err());
        model.target_os = None;
        model.target_arch = None;
        assert!(model.validate_shape().is_ok());
    }
}
