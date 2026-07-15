//! Signed handler-pack (`.rillhandler`) construction and verification.

use std::io::{Read, Seek};

use ed25519_dalek::SigningKey;
use rill_runtime_protocol::HandlerPackManifest;
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::archive::{
    ArchiveError, ArchiveLimits, DEFAULT_PATHS, TrustStore, build_signed_archive, read_archive,
    verify_checksums_and_signature,
};

const MODULE_PATH: &str = "handler.wasm";

const HANDLER_PACK_LIMITS: ArchiveLimits = ArchiveLimits {
    max_files: 8,
    max_file_bytes: 4 * 1024 * 1024,
    max_total_bytes: 16 * 1024 * 1024,
    max_compression_ratio: 100,
};

const HANDLER_PACK_ALLOWED: &[&str] = &[
    "manifest.json",
    MODULE_PATH,
    "checksums.json",
    "META-INF/signature.ed25519",
];

#[derive(Debug, Clone)]
pub struct LoadedHandlerPack {
    pub manifest: HandlerPackManifest,
    pub module: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandlerPackInspection {
    pub id: String,
    pub version: String,
    pub publisher_key_id: String,
    pub handler_api_version: u32,
    pub min_runtime_version: String,
    pub capabilities: Vec<String>,
    pub module_sha256: String,
    pub module_size: u64,
    pub signature_verified: bool,
}

#[derive(Debug, Error)]
pub enum HandlerPackError {
    #[error(transparent)]
    Archive(#[from] ArchiveError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("invalid handler manifest: {0}")]
    Manifest(String),
    #[error("runtime {actual} is older than handler requirement {minimum}")]
    RuntimeTooOld { minimum: String, actual: String },
    #[error("handler module SHA-256 mismatch")]
    ModuleDigestMismatch,
}

pub fn load_handler_pack<R: Read + Seek>(
    reader: R,
    trust: &TrustStore,
) -> Result<(LoadedHandlerPack, HandlerPackInspection), HandlerPackError> {
    let files = read_archive(reader, HANDLER_PACK_ALLOWED, HANDLER_PACK_LIMITS)?;
    let manifest_bytes = files
        .get(DEFAULT_PATHS.manifest)
        .ok_or(ArchiveError::Missing(DEFAULT_PATHS.manifest))?;
    let manifest: HandlerPackManifest = serde_json::from_slice(manifest_bytes)?;
    manifest
        .validate_shape()
        .map_err(|message| HandlerPackError::Manifest(message.into()))?;
    semver::Version::parse(&manifest.version)
        .map_err(|error| HandlerPackError::Manifest(format!("invalid handler version: {error}")))?;
    let minimum = semver::Version::parse(&manifest.min_runtime_version)
        .map_err(|error| HandlerPackError::Manifest(format!("invalid minimum runtime: {error}")))?;
    let runtime = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| HandlerPackError::Manifest(format!("invalid runtime version: {error}")))?;
    if runtime < minimum {
        return Err(HandlerPackError::RuntimeTooOld {
            minimum: minimum.to_string(),
            actual: runtime.to_string(),
        });
    }
    verify_checksums_and_signature(
        &files,
        &DEFAULT_PATHS,
        &[DEFAULT_PATHS.manifest, MODULE_PATH],
        &manifest.publisher_key_id,
        trust,
    )?;

    let module = files
        .get(MODULE_PATH)
        .ok_or(ArchiveError::Missing(MODULE_PATH))?
        .clone();

    // Verify the manifest's moduleSha256 matches the actual module bytes.
    let actual_digest = hex::encode(Sha256::digest(&module));
    if actual_digest != manifest.module_sha256 {
        return Err(HandlerPackError::ModuleDigestMismatch);
    }
    if module.len() as u64 != manifest.module_size {
        return Err(HandlerPackError::Manifest("module size mismatch".into()));
    }

    let inspection = HandlerPackInspection {
        id: manifest.id.clone(),
        version: manifest.version.clone(),
        publisher_key_id: manifest.publisher_key_id.clone(),
        handler_api_version: manifest.handler_api_version,
        min_runtime_version: manifest.min_runtime_version.clone(),
        capabilities: manifest.capabilities.clone(),
        module_sha256: manifest.module_sha256.clone(),
        module_size: manifest.module_size,
        signature_verified: true,
    };
    Ok((LoadedHandlerPack { manifest, module }, inspection))
}

pub fn build_signed_handler_pack(
    manifest: &HandlerPackManifest,
    module: &[u8],
    signing_key: &SigningKey,
) -> Result<Vec<u8>, HandlerPackError> {
    manifest
        .validate_shape()
        .map_err(|message| HandlerPackError::Manifest(message.into()))?;
    // Verify moduleSha256 and moduleSize match the actual module.
    let actual_digest = hex::encode(Sha256::digest(module));
    if actual_digest != manifest.module_sha256 {
        return Err(HandlerPackError::ModuleDigestMismatch);
    }
    if module.len() as u64 != manifest.module_size {
        return Err(HandlerPackError::Manifest("module size mismatch".into()));
    }
    let manifest_bytes = serde_json::to_vec_pretty(manifest)?;
    let archive = build_signed_archive(&manifest_bytes, MODULE_PATH, module, signing_key)?;
    Ok(archive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_runtime_protocol::{HANDLER_API_VERSION, HANDLER_PACKAGE_FORMAT_VERSION};
    use std::collections::BTreeMap;

    fn manifest(key_id: &str, module: &[u8]) -> HandlerPackManifest {
        HandlerPackManifest {
            format_version: HANDLER_PACKAGE_FORMAT_VERSION,
            id: "org.example.handler".into(),
            version: "1.0.0".into(),
            handler_api_version: HANDLER_API_VERSION,
            min_runtime_version: "0.7.0".into(),
            publisher_key_id: key_id.into(),
            capabilities: vec!["org.example.predict".into()],
            module_sha256: hex::encode(Sha256::digest(module)),
            module_size: module.len() as u64,
        }
    }

    #[test]
    fn handler_pack_roundtrip_and_tamper_rejection() {
        let signing = SigningKey::from_bytes(&[11; 32]);
        let key_id = "handler-test-key";
        let module = b"\0asm\x01\x00\x00\x00test module bytes";
        let bytes = build_signed_handler_pack(&manifest(key_id, module), module, &signing).unwrap();
        let trust = TrustStore(BTreeMap::from([(key_id.into(), signing.verifying_key())]));
        let (loaded, inspection) = load_handler_pack(std::io::Cursor::new(&bytes), &trust).unwrap();
        assert_eq!(loaded.manifest.id, "org.example.handler");
        assert_eq!(loaded.module, module);
        assert!(inspection.signature_verified);

        let wrong = SigningKey::from_bytes(&[12; 32]);
        let wrong_trust = TrustStore(BTreeMap::from([(key_id.into(), wrong.verifying_key())]));
        assert!(matches!(
            load_handler_pack(std::io::Cursor::new(bytes), &wrong_trust),
            Err(HandlerPackError::Archive(ArchiveError::Signature))
        ));
    }

    #[test]
    fn handler_pack_rejects_wrong_module_digest() {
        let signing = SigningKey::from_bytes(&[13; 32]);
        let key_id = "digest-test";
        let real_module = b"real module";
        let fake_module = b"fake module";
        let mut bad_manifest = manifest(key_id, fake_module);
        // Set the digest to the real module's digest, but provide the fake module.
        bad_manifest.module_sha256 = hex::encode(Sha256::digest(real_module));
        bad_manifest.module_size = real_module.len() as u64;
        assert!(matches!(
            build_signed_handler_pack(&bad_manifest, fake_module, &signing),
            Err(HandlerPackError::ModuleDigestMismatch)
        ));
    }

    #[test]
    fn handler_pack_rejects_model_trust_key() {
        // A handler signed with one key must not verify against a different key.
        let handler_signing = SigningKey::from_bytes(&[14; 32]);
        let model_signing = SigningKey::from_bytes(&[15; 32]);
        let key_id = "shared-key-id";
        let module = b"test module";
        let bytes =
            build_signed_handler_pack(&manifest(key_id, module), module, &handler_signing).unwrap();
        // Model trust store uses a different key under the same key id.
        let model_trust = TrustStore(BTreeMap::from([(
            key_id.into(),
            model_signing.verifying_key(),
        )]));
        assert!(matches!(
            load_handler_pack(std::io::Cursor::new(&bytes), &model_trust),
            Err(HandlerPackError::Archive(ArchiveError::Signature))
        ));
    }
}
