//! Signed model-pack (`.rillpack`) construction and verification.

use std::io::{Read, Seek};

use ed25519_dalek::SigningKey;
use rill_runtime_protocol::ModelPackManifest;
use serde::Serialize;
use thiserror::Error;

use crate::archive::{
    ArchiveError, ArchiveLimits, DEFAULT_PATHS, TrustStore, build_signed_archive, canonical_json,
    read_archive, verify_checksums_and_signature,
};

pub use crate::archive::{ReleaseIndexError, sign_release_index, verify_release_index};

const MODEL_PATH: &str = "model.json";

const MODEL_PACK_LIMITS: ArchiveLimits = ArchiveLimits {
    max_files: 8,
    max_file_bytes: 256 * 1024,
    max_total_bytes: 1024 * 1024,
    max_compressed_total_bytes: 8 * 1024 * 1024,
    max_compression_ratio: 100,
};

const MODEL_PACK_ALLOWED: &[&str] = &[
    "manifest.json",
    MODEL_PATH,
    "checksums.json",
    "META-INF/signature.ed25519",
];

#[derive(Debug, Clone)]
pub struct LoadedModelPack {
    pub manifest: ModelPackManifest,
    pub model: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPackInspection {
    pub id: String,
    pub version: String,
    pub publisher_key_id: String,
    pub runtime_api_version: u32,
    pub capabilities: Vec<String>,
    pub signature_verified: bool,
}

#[derive(Debug, Error)]
pub enum ModelPackError {
    #[error(transparent)]
    Archive(#[from] ArchiveError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("invalid model manifest: {0}")]
    Manifest(String),
    #[error("runtime {actual} is older than model requirement {minimum}")]
    RuntimeTooOld { minimum: String, actual: String },
}

pub fn load_model_pack<R: Read + Seek>(
    reader: R,
    trust: &TrustStore,
) -> Result<(LoadedModelPack, ModelPackInspection), ModelPackError> {
    let files = read_archive(reader, MODEL_PACK_ALLOWED, MODEL_PACK_LIMITS)?;
    let manifest_bytes = files
        .get(DEFAULT_PATHS.manifest)
        .ok_or(ArchiveError::Missing(DEFAULT_PATHS.manifest))?;
    let manifest: ModelPackManifest = serde_json::from_slice(manifest_bytes)?;
    manifest
        .validate_shape()
        .map_err(|message| ModelPackError::Manifest(message.into()))?;
    semver::Version::parse(&manifest.version)
        .map_err(|error| ModelPackError::Manifest(format!("invalid pack version: {error}")))?;
    let minimum = semver::Version::parse(&manifest.min_runtime_version)
        .map_err(|error| ModelPackError::Manifest(format!("invalid minimum runtime: {error}")))?;
    let runtime = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| ModelPackError::Manifest(format!("invalid runtime version: {error}")))?;
    if runtime < minimum {
        return Err(ModelPackError::RuntimeTooOld {
            minimum: minimum.to_string(),
            actual: runtime.to_string(),
        });
    }
    verify_checksums_and_signature(
        &files,
        &DEFAULT_PATHS,
        &[DEFAULT_PATHS.manifest, MODEL_PATH],
        &manifest.publisher_key_id,
        trust,
    )?;

    let model: serde_json::Value = serde_json::from_slice(
        files
            .get(MODEL_PATH)
            .ok_or(ArchiveError::Missing(MODEL_PATH))?,
    )?;
    let inspection = ModelPackInspection {
        id: manifest.id.clone(),
        version: manifest.version.clone(),
        publisher_key_id: manifest.publisher_key_id.clone(),
        runtime_api_version: manifest.runtime_api_version,
        capabilities: manifest.capabilities.clone(),
        signature_verified: true,
    };
    Ok((LoadedModelPack { manifest, model }, inspection))
}

pub fn build_signed_model_pack(
    manifest: &ModelPackManifest,
    model: &serde_json::Value,
    signing_key: &SigningKey,
) -> Result<Vec<u8>, ModelPackError> {
    manifest
        .validate_shape()
        .map_err(|message| ModelPackError::Manifest(message.into()))?;
    let manifest_bytes = serde_json::to_vec_pretty(manifest)?;
    let model_bytes = serde_json::to_vec_pretty(model)?;
    let _ = canonical_json(&manifest_bytes)?;
    let archive = build_signed_archive(&manifest_bytes, MODEL_PATH, &model_bytes, signing_key)?;
    Ok(archive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_runtime_protocol::{MODEL_PACK_FORMAT_VERSION, RUNTIME_API_VERSION};
    use std::collections::BTreeMap;

    fn manifest(key_id: &str) -> ModelPackManifest {
        ModelPackManifest {
            format_version: MODEL_PACK_FORMAT_VERSION,
            id: "rillml.example.default".into(),
            version: "0.7.0".into(),
            runtime_api_version: RUNTIME_API_VERSION,
            min_runtime_version: "0.7.0".into(),
            publisher_key_id: key_id.into(),
            capabilities: vec!["rillml.example".into()],
        }
    }

    #[test]
    fn signed_pack_roundtrip_and_tamper_rejection() {
        let signing = SigningKey::from_bytes(&[7; 32]);
        let key_id = "test-key";
        let bytes = build_signed_model_pack(
            &manifest(key_id),
            &serde_json::json!({"description": "test"}),
            &signing,
        )
        .unwrap();
        let trust = TrustStore(BTreeMap::from([(key_id.into(), signing.verifying_key())]));
        let (loaded, inspection) = load_model_pack(std::io::Cursor::new(&bytes), &trust).unwrap();
        assert_eq!(loaded.manifest.id, "rillml.example.default");
        assert!(inspection.signature_verified);

        let wrong = SigningKey::from_bytes(&[8; 32]);
        let wrong_trust = TrustStore(BTreeMap::from([(key_id.into(), wrong.verifying_key())]));
        assert!(matches!(
            load_model_pack(std::io::Cursor::new(bytes), &wrong_trust),
            Err(ModelPackError::Archive(ArchiveError::Signature))
        ));
    }

    #[test]
    fn release_index_signature_covers_artifact_hashes() {
        use rill_runtime_protocol::{
            RELEASE_INDEX_SCHEMA_VERSION, RUNTIME_ARTIFACT_ID, ReleaseArtifact,
            ReleaseArtifactKind, ReleaseIndexPayload,
        };

        let signing = SigningKey::from_bytes(&[6; 32]);
        let payload = ReleaseIndexPayload {
            schema_version: RELEASE_INDEX_SCHEMA_VERSION,
            channel: "stable".into(),
            generated_at: "2026-07-15T00:00:00Z".into(),
            publisher_key_id: "release-test".into(),
            artifacts: vec![ReleaseArtifact {
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
            }],
        };
        let mut index = sign_release_index(payload, &signing).unwrap();
        let trust = TrustStore(BTreeMap::from([(
            "release-test".into(),
            signing.verifying_key(),
        )]));
        verify_release_index(&index, &trust).unwrap();
        index.payload.artifacts[0].sha256 = "cd".repeat(32);
        assert!(matches!(
            verify_release_index(&index, &trust),
            Err(ReleaseIndexError::Signature)
        ));
    }

    #[test]
    fn release_index_supports_handler_artifact() {
        use rill_runtime_protocol::{
            HANDLER_API_VERSION, RELEASE_INDEX_SCHEMA_VERSION, ReleaseArtifact,
            ReleaseArtifactKind, ReleaseIndexPayload,
        };

        let signing = SigningKey::from_bytes(&[9; 32]);
        let payload = ReleaseIndexPayload {
            schema_version: RELEASE_INDEX_SCHEMA_VERSION,
            channel: "stable".into(),
            generated_at: "2026-07-15T00:00:00Z".into(),
            publisher_key_id: "release-test".into(),
            artifacts: vec![ReleaseArtifact {
                kind: ReleaseArtifactKind::Handler,
                id: "org.example.handler".into(),
                version: "1.0.0".into(),
                runtime_api_version: RUNTIME_API_VERSION,
                target_os: None,
                target_arch: None,
                handler_api_version: Some(HANDLER_API_VERSION),
                min_runtime_version: Some("0.7.0".into()),
                url: "https://example.invalid/handler.wasm".into(),
                sha256: "ef".repeat(32),
                size: 2048,
            }],
        };
        let index = sign_release_index(payload, &signing).unwrap();
        let trust = TrustStore(BTreeMap::from([(
            "release-test".into(),
            signing.verifying_key(),
        )]));
        assert!(verify_release_index(&index, &trust).is_ok());
    }

    // ----- R-021: compatibility tests -----

    #[test]
    fn release_index_rejects_v1_schema() {
        // A release index using the legacy v1 schema (schema_version=1) must be
        // rejected. Only schema_version=2 (RELEASE_INDEX_SCHEMA_VERSION) is
        // accepted by the current runtime.
        use rill_runtime_protocol::{
            RUNTIME_API_VERSION, RUNTIME_ARTIFACT_ID, ReleaseArtifact, ReleaseArtifactKind,
            ReleaseIndexPayload,
        };

        let signing = SigningKey::from_bytes(&[42; 32]);
        let payload = ReleaseIndexPayload {
            schema_version: 1, // legacy v1 schema
            channel: "stable".into(),
            generated_at: "2026-07-15T00:00:00Z".into(),
            publisher_key_id: "v1-schema-test".into(),
            artifacts: vec![ReleaseArtifact {
                kind: ReleaseArtifactKind::Runtime,
                id: RUNTIME_ARTIFACT_ID.into(),
                version: "0.6.0".into(),
                runtime_api_version: RUNTIME_API_VERSION,
                target_os: Some("macos".into()),
                target_arch: Some("aarch64".into()),
                handler_api_version: None,
                min_runtime_version: None,
                url: "https://example.invalid/rill-runtime".into(),
                sha256: "ab".repeat(32),
                size: 1024,
            }],
        };
        // sign_release_index calls validate_release_payload which rejects v1.
        assert!(sign_release_index(payload, &signing).is_err());
    }
}
