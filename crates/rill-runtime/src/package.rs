use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Seek, Write},
};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rill_runtime_protocol::{
    BATTERY_USAGE_CAPABILITY, BatteryModelConfig, ModelPackManifest, ReleaseIndexPayload,
    SignedReleaseIndex,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const MAX_FILES: usize = 8;
const MAX_FILE_BYTES: u64 = 256 * 1024;
const MAX_TOTAL_BYTES: u64 = 1024 * 1024;

const MANIFEST_PATH: &str = "manifest.json";
const MODEL_PATH: &str = "model.json";
const CHECKSUMS_PATH: &str = "checksums.json";
const SIGNATURE_PATH: &str = "META-INF/signature.ed25519";

#[derive(Debug, Default, Clone)]
pub struct TrustStore(pub BTreeMap<String, VerifyingKey>);

#[derive(Debug, Clone)]
pub struct LoadedModelPack {
    pub manifest: ModelPackManifest,
    pub battery: BatteryModelConfig,
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Checksums {
    schema_version: u32,
    files: BTreeMap<String, String>,
}

pub fn canonical_json(bytes: &[u8]) -> Result<Vec<u8>, ModelPackError> {
    fn sort(value: Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(key, value)| (key, sort(value)))
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.into_iter().map(sort).collect()),
            other => other,
        }
    }
    let value: Value = serde_json::from_slice(bytes)?;
    Ok(serde_json::to_vec(&sort(value))?)
}

pub fn sign_release_index(
    payload: ReleaseIndexPayload,
    signing_key: &SigningKey,
) -> Result<SignedReleaseIndex, ReleaseIndexError> {
    validate_release_payload(&payload)?;
    let serialized = serde_json::to_vec(&payload)?;
    let canonical = canonical_json(&serialized).map_err(ReleaseIndexError::Canonical)?;
    let signature = hex::encode(signing_key.sign(&canonical).to_bytes());
    Ok(SignedReleaseIndex { payload, signature })
}

pub fn verify_release_index(
    index: &SignedReleaseIndex,
    trust: &TrustStore,
) -> Result<(), ReleaseIndexError> {
    validate_release_payload(&index.payload)?;
    let signature_bytes =
        hex::decode(&index.signature).map_err(|_| ReleaseIndexError::Signature)?;
    let signature =
        Signature::from_slice(&signature_bytes).map_err(|_| ReleaseIndexError::Signature)?;
    let key = trust
        .0
        .get(&index.payload.publisher_key_id)
        .ok_or(ReleaseIndexError::UnknownKey)?;
    let serialized = serde_json::to_vec(&index.payload)?;
    let canonical = canonical_json(&serialized).map_err(ReleaseIndexError::Canonical)?;
    key.verify(&canonical, &signature)
        .map_err(|_| ReleaseIndexError::Signature)
}

fn validate_release_payload(payload: &ReleaseIndexPayload) -> Result<(), ReleaseIndexError> {
    payload
        .validate_shape()
        .map_err(|message| ReleaseIndexError::Manifest(message.into()))?;
    let mut identities = std::collections::BTreeSet::new();
    for artifact in &payload.artifacts {
        semver::Version::parse(&artifact.version).map_err(|error| {
            ReleaseIndexError::Manifest(format!("invalid artifact version: {error}"))
        })?;
        let identity = (
            artifact.kind.clone(),
            artifact.id.clone(),
            artifact.target_os.clone(),
            artifact.target_arch.clone(),
        );
        if !identities.insert(identity) {
            return Err(ReleaseIndexError::Manifest(
                "duplicate release artifact identity".into(),
            ));
        }
    }
    Ok(())
}

pub fn load_model_pack<R: Read + Seek>(
    reader: R,
    trust: &TrustStore,
) -> Result<(LoadedModelPack, ModelPackInspection), ModelPackError> {
    let files = read_archive(reader)?;
    let manifest_bytes = files
        .get(MANIFEST_PATH)
        .ok_or(ModelPackError::Missing(MANIFEST_PATH))?;
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
    if !manifest
        .capabilities
        .iter()
        .any(|capability| capability == BATTERY_USAGE_CAPABILITY)
    {
        return Err(ModelPackError::Manifest(
            "batteryUsage capability is required".into(),
        ));
    }

    verify_checksums_and_signature(&files, &manifest, trust)?;

    let battery: BatteryModelConfig = serde_json::from_slice(
        files
            .get(MODEL_PATH)
            .ok_or(ModelPackError::Missing(MODEL_PATH))?,
    )?;
    battery
        .validate()
        .map_err(|message| ModelPackError::Manifest(message.into()))?;
    let inspection = ModelPackInspection {
        id: manifest.id.clone(),
        version: manifest.version.clone(),
        publisher_key_id: manifest.publisher_key_id.clone(),
        runtime_api_version: manifest.runtime_api_version,
        capabilities: manifest.capabilities.clone(),
        signature_verified: true,
    };
    Ok((LoadedModelPack { manifest, battery }, inspection))
}

fn read_archive<R: Read + Seek>(reader: R) -> Result<BTreeMap<String, Vec<u8>>, ModelPackError> {
    let mut archive = ZipArchive::new(reader)?;
    if archive.len() > MAX_FILES {
        return Err(ModelPackError::Limit("file count"));
    }
    let mut total = 0u64;
    let mut files = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        validate_path(&name)?;
        if !matches!(
            name.as_str(),
            MANIFEST_PATH | MODEL_PATH | CHECKSUMS_PATH | SIGNATURE_PATH
        ) {
            return Err(ModelPackError::Forbidden(name));
        }
        if entry.size() > MAX_FILE_BYTES {
            return Err(ModelPackError::Limit("file size"));
        }
        total = total
            .checked_add(entry.size())
            .ok_or(ModelPackError::Limit("total size"))?;
        if total > MAX_TOTAL_BYTES {
            return Err(ModelPackError::Limit("total size"));
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes)?;
        if files.insert(name.clone(), bytes).is_some() {
            return Err(ModelPackError::Duplicate(name));
        }
    }
    Ok(files)
}

fn verify_checksums_and_signature(
    files: &BTreeMap<String, Vec<u8>>,
    manifest: &ModelPackManifest,
    trust: &TrustStore,
) -> Result<(), ModelPackError> {
    let checksum_bytes = files
        .get(CHECKSUMS_PATH)
        .ok_or(ModelPackError::Missing(CHECKSUMS_PATH))?;
    let checksums: Checksums = serde_json::from_slice(checksum_bytes)?;
    if checksums.schema_version != 1 {
        return Err(ModelPackError::Manifest(
            "unsupported checksum schema".into(),
        ));
    }
    let expected_names = vec![MANIFEST_PATH.to_string(), MODEL_PATH.to_string()];
    if checksums.files.keys().cloned().collect::<Vec<_>>() != expected_names {
        return Err(ModelPackError::ChecksumCoverage);
    }
    for (name, expected) in &checksums.files {
        let bytes = files
            .get(name)
            .ok_or_else(|| ModelPackError::MissingOwned(name.clone()))?;
        let actual = hex::encode(Sha256::digest(bytes));
        if &actual != expected {
            return Err(ModelPackError::Digest(name.clone()));
        }
    }
    let raw_signature = files
        .get(SIGNATURE_PATH)
        .ok_or(ModelPackError::Missing(SIGNATURE_PATH))?;
    let signature = Signature::from_slice(raw_signature).map_err(|_| ModelPackError::Signature)?;
    let key = trust
        .0
        .get(&manifest.publisher_key_id)
        .ok_or(ModelPackError::UnknownKey)?;
    let mut message = canonical_json(
        files
            .get(MANIFEST_PATH)
            .ok_or(ModelPackError::Missing(MANIFEST_PATH))?,
    )?;
    message.push(b'\n');
    message.extend(canonical_json(checksum_bytes)?);
    key.verify(&message, &signature)
        .map_err(|_| ModelPackError::Signature)
}

pub fn build_signed_model_pack(
    manifest: &ModelPackManifest,
    battery: &BatteryModelConfig,
    signing_key: &SigningKey,
) -> Result<Vec<u8>, ModelPackError> {
    manifest
        .validate_shape()
        .map_err(|message| ModelPackError::Manifest(message.into()))?;
    battery
        .validate()
        .map_err(|message| ModelPackError::Manifest(message.into()))?;
    let manifest_bytes = serde_json::to_vec_pretty(manifest)?;
    let model_bytes = serde_json::to_vec_pretty(battery)?;
    let checksums = Checksums {
        schema_version: 1,
        files: BTreeMap::from([
            (
                MANIFEST_PATH.into(),
                hex::encode(Sha256::digest(&manifest_bytes)),
            ),
            (MODEL_PATH.into(), hex::encode(Sha256::digest(&model_bytes))),
        ]),
    };
    let checksum_bytes = serde_json::to_vec_pretty(&checksums)?;
    let mut message = canonical_json(&manifest_bytes)?;
    message.push(b'\n');
    message.extend(canonical_json(&checksum_bytes)?);
    let signature = signing_key.sign(&message).to_bytes();

    let mut output = Cursor::new(Vec::new());
    {
        let mut archive = ZipWriter::new(&mut output);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        for (name, bytes) in [
            (MANIFEST_PATH, manifest_bytes.as_slice()),
            (MODEL_PATH, model_bytes.as_slice()),
            (CHECKSUMS_PATH, checksum_bytes.as_slice()),
            (SIGNATURE_PATH, signature.as_slice()),
        ] {
            archive.start_file(name, options)?;
            archive.write_all(bytes)?;
        }
        archive.finish()?;
    }
    Ok(output.into_inner())
}

fn validate_path(name: &str) -> Result<(), ModelPackError> {
    if name.starts_with('/')
        || name.contains('\\')
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ModelPackError::UnsafePath(name.into()));
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum ModelPackError {
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsafe package path {0}")]
    UnsafePath(String),
    #[error("forbidden package file {0}")]
    Forbidden(String),
    #[error("duplicate package file {0}")]
    Duplicate(String),
    #[error("model package exceeded {0} limit")]
    Limit(&'static str),
    #[error("missing package file {0}")]
    Missing(&'static str),
    #[error("missing package file {0}")]
    MissingOwned(String),
    #[error("invalid model manifest: {0}")]
    Manifest(String),
    #[error("checksum coverage does not exactly match the model payload")]
    ChecksumCoverage,
    #[error("checksum mismatch for {0}")]
    Digest(String),
    #[error("unknown publisher key")]
    UnknownKey,
    #[error("signature verification failed")]
    Signature,
    #[error("runtime {actual} is older than model requirement {minimum}")]
    RuntimeTooOld { minimum: String, actual: String },
}

#[derive(Debug, Error)]
pub enum ReleaseIndexError {
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid release index: {0}")]
    Manifest(String),
    #[error("unknown release-index publisher key")]
    UnknownKey,
    #[error("release-index signature verification failed")]
    Signature,
    #[error("canonical JSON error: {0}")]
    Canonical(ModelPackError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_runtime_protocol::{MODEL_PACK_FORMAT_VERSION, RUNTIME_API_VERSION};

    fn manifest(key_id: &str) -> ModelPackManifest {
        ModelPackManifest {
            format_version: MODEL_PACK_FORMAT_VERSION,
            id: "mira.battery.default".into(),
            version: "0.5.0".into(),
            runtime_api_version: RUNTIME_API_VERSION,
            min_runtime_version: "0.5.0".into(),
            publisher_key_id: key_id.into(),
            capabilities: vec![BATTERY_USAGE_CAPABILITY.into()],
        }
    }

    #[test]
    fn signed_pack_roundtrip_and_tamper_rejection() {
        let signing = SigningKey::from_bytes(&[7; 32]);
        let key_id = "test-key";
        let bytes =
            build_signed_model_pack(&manifest(key_id), &BatteryModelConfig::default(), &signing)
                .unwrap();
        let trust = TrustStore(BTreeMap::from([(key_id.into(), signing.verifying_key())]));
        let (loaded, inspection) = load_model_pack(Cursor::new(&bytes), &trust).unwrap();
        assert_eq!(loaded.manifest.id, "mira.battery.default");
        assert!(inspection.signature_verified);

        let wrong = SigningKey::from_bytes(&[8; 32]);
        let wrong_trust = TrustStore(BTreeMap::from([(key_id.into(), wrong.verifying_key())]));
        assert!(matches!(
            load_model_pack(Cursor::new(bytes), &wrong_trust),
            Err(ModelPackError::Signature)
        ));
    }

    #[test]
    fn config_validation_happens_after_signature_verification() {
        let signing = SigningKey::from_bytes(&[9; 32]);
        let config = BatteryModelConfig {
            quality_window: 0,
            ..BatteryModelConfig::default()
        };
        assert!(build_signed_model_pack(&manifest("test"), &config, &signing).is_err());
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
            generated_at: "2026-07-13T00:00:00Z".into(),
            publisher_key_id: "release-test".into(),
            artifacts: vec![ReleaseArtifact {
                kind: ReleaseArtifactKind::Runtime,
                id: RUNTIME_ARTIFACT_ID.into(),
                version: "0.5.0".into(),
                runtime_api_version: RUNTIME_API_VERSION,
                target_os: Some("macos".into()),
                target_arch: Some("aarch64".into()),
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
}
