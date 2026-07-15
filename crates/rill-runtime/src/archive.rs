//! Common safe-archive skeleton shared by model packs (`.rillpack`) and
//! handler packs (`.rillhandler`).
//!
//! Both pack formats use the same ZIP structure: a manifest, one payload file,
//! a checksums file, and an Ed25519 signature. This module centralises the
//! path validation, size limits, checksum verification and signature logic so
//! that the two pack types cannot drift apart.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read, Seek, Write},
};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rill_runtime_protocol::ReleaseIndexPayload;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const MANIFEST_PATH: &str = "manifest.json";
const CHECKSUMS_PATH: &str = "checksums.json";
const SIGNATURE_PATH: &str = "META-INF/signature.ed25519";

#[derive(Debug, Default, Clone)]
pub struct TrustStore(pub BTreeMap<String, VerifyingKey>);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Checksums {
    schema_version: u32,
    files: BTreeMap<String, String>,
}

#[derive(Debug, Error)]
pub enum ArchiveError {
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
    #[error("package exceeded {0} limit")]
    Limit(&'static str),
    #[error("missing package file {0}")]
    Missing(&'static str),
    #[error("missing package file {0}")]
    MissingOwned(String),
    #[error("checksum coverage does not exactly match the payload")]
    ChecksumCoverage,
    #[error("checksum mismatch for {0}")]
    Digest(String),
    #[error("unknown publisher key")]
    UnknownKey,
    #[error("signature verification failed")]
    Signature,
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
    Canonical(ArchiveError),
}

/// Limits for a specific pack type.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ArchiveLimits {
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub max_compression_ratio: u64,
}

/// The canonical paths every pack must contain.
pub(crate) struct PackPaths {
    pub manifest: &'static str,
    pub checksums: &'static str,
    pub signature: &'static str,
}

pub(crate) const DEFAULT_PATHS: PackPaths = PackPaths {
    manifest: MANIFEST_PATH,
    checksums: CHECKSUMS_PATH,
    signature: SIGNATURE_PATH,
};

pub fn canonical_json(bytes: &[u8]) -> Result<Vec<u8>, ArchiveError> {
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
) -> Result<rill_runtime_protocol::SignedReleaseIndex, ReleaseIndexError> {
    validate_release_payload(&payload)?;
    let serialized = serde_json::to_vec(&payload)?;
    let canonical = canonical_json(&serialized).map_err(ReleaseIndexError::Canonical)?;
    let signature = hex::encode(signing_key.sign(&canonical).to_bytes());
    Ok(rill_runtime_protocol::SignedReleaseIndex { payload, signature })
}

pub fn verify_release_index(
    index: &rill_runtime_protocol::SignedReleaseIndex,
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
    let mut identities = BTreeSet::new();
    for artifact in &payload.artifacts {
        semver::Version::parse(&artifact.version).map_err(|error| {
            ReleaseIndexError::Manifest(format!("invalid artifact version: {error}"))
        })?;
        let identity = (
            artifact.kind.clone(),
            artifact.id.clone(),
            artifact.target_os.clone(),
            artifact.target_arch.clone(),
            artifact.handler_api_version,
        );
        if !identities.insert(identity) {
            return Err(ReleaseIndexError::Manifest(
                "duplicate release artifact identity".into(),
            ));
        }
    }
    Ok(())
}

/// Read a ZIP archive and validate paths, file count, and size limits.
/// Returns a map of file name → bytes for every non-directory entry.
pub(crate) fn read_archive<R: Read + Seek>(
    reader: R,
    allowed: &[&str],
    limits: ArchiveLimits,
) -> Result<BTreeMap<String, Vec<u8>>, ArchiveError> {
    let mut archive = ZipArchive::new(reader)?;
    if archive.len() > limits.max_files {
        return Err(ArchiveError::Limit("file count"));
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
        if !allowed.iter().any(|allowed| *allowed == name) {
            return Err(ArchiveError::Forbidden(name));
        }
        if entry.size() > limits.max_file_bytes {
            return Err(ArchiveError::Limit("file size"));
        }
        let compressed = entry.compressed_size();
        if compressed > 0 && entry.size() / compressed > limits.max_compression_ratio {
            return Err(ArchiveError::Limit("compression ratio"));
        }
        total = total
            .checked_add(entry.size())
            .ok_or(ArchiveError::Limit("total size"))?;
        if total > limits.max_total_bytes {
            return Err(ArchiveError::Limit("total size"));
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes)?;
        if files.insert(name.clone(), bytes).is_some() {
            return Err(ArchiveError::Duplicate(name));
        }
    }
    Ok(files)
}

/// Verify checksums and signature for a pack.
///
/// `checksum_files` lists the payload file names that checksums.json must
/// cover, in canonical order.
pub(crate) fn verify_checksums_and_signature(
    files: &BTreeMap<String, Vec<u8>>,
    paths: &PackPaths,
    checksum_payload_names: &[&str],
    publisher_key_id: &str,
    trust: &TrustStore,
) -> Result<(), ArchiveError> {
    let checksum_bytes = files
        .get(paths.checksums)
        .ok_or(ArchiveError::Missing(paths.checksums))?;
    let checksums: Checksums = serde_json::from_slice(checksum_bytes)?;
    if checksums.schema_version != 1 {
        return Err(ArchiveError::Missing("checksum schema version"));
    }
    let mut expected_names: Vec<String> = checksum_payload_names
        .iter()
        .map(|s| s.to_string())
        .collect();
    expected_names.sort();
    let actual_names: Vec<String> = checksums.files.keys().cloned().collect();
    if actual_names != expected_names {
        return Err(ArchiveError::ChecksumCoverage);
    }
    for (name, expected) in &checksums.files {
        let bytes = files
            .get(name)
            .ok_or_else(|| ArchiveError::MissingOwned(name.clone()))?;
        let actual = hex::encode(Sha256::digest(bytes));
        if &actual != expected {
            return Err(ArchiveError::Digest(name.clone()));
        }
    }
    let raw_signature = files
        .get(paths.signature)
        .ok_or(ArchiveError::Missing(paths.signature))?;
    let signature = Signature::from_slice(raw_signature).map_err(|_| ArchiveError::Signature)?;
    let key = trust
        .0
        .get(publisher_key_id)
        .ok_or(ArchiveError::UnknownKey)?;
    let manifest_bytes = files
        .get(paths.manifest)
        .ok_or(ArchiveError::Missing(paths.manifest))?;
    let mut message = canonical_json(manifest_bytes)?;
    message.push(b'\n');
    message.extend(canonical_json(checksum_bytes)?);
    key.verify(&message, &signature)
        .map_err(|_| ArchiveError::Signature)
}

/// Build a signed ZIP archive from manifest bytes, payload bytes, and a
/// signing key. Returns the complete archive bytes.
pub(crate) fn build_signed_archive(
    manifest_bytes: &[u8],
    payload_name: &str,
    payload_bytes: &[u8],
    signing_key: &SigningKey,
) -> Result<Vec<u8>, ArchiveError> {
    let checksums = Checksums {
        schema_version: 1,
        files: BTreeMap::from([
            (
                MANIFEST_PATH.into(),
                hex::encode(Sha256::digest(manifest_bytes)),
            ),
            (
                payload_name.into(),
                hex::encode(Sha256::digest(payload_bytes)),
            ),
        ]),
    };
    let checksum_bytes = serde_json::to_vec_pretty(&checksums)?;
    let mut message = canonical_json(manifest_bytes)?;
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
            (MANIFEST_PATH, manifest_bytes),
            (payload_name, payload_bytes),
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

fn validate_path(name: &str) -> Result<(), ArchiveError> {
    if name.starts_with('/')
        || name.contains('\\')
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ArchiveError::UnsafePath(name.into()));
    }
    Ok(())
}
