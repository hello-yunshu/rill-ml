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
use rill_runtime_protocol::{
    ReleaseIndexPayload, SignedReleaseIndexWithGenerationV1, TrustMetadataV1,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

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
#[non_exhaustive]
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
    #[error("package member size differs from ZIP metadata for {0}")]
    SizeMismatch(String),
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
#[non_exhaustive]
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
    #[error("invalid trust metadata: {0}")]
    TrustMetadata(String),
    #[error("trust metadata generation is older than the consumer metadata floor")]
    MetadataDowngrade,
    #[error("trust metadata content conflicts with the consumer metadata floor")]
    MetadataConflict,
    #[error("release index generation is older than the consumer rollback floor")]
    Downgrade,
}

/// Limits for a specific pack type.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ArchiveLimits {
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub max_compressed_total_bytes: u64,
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
    fn canonical(value: Value) -> Value {
        match value {
            Value::Object(map) => {
                // Explicitly sort object keys via BTreeMap so canonicalisation
                // does not depend on serde_json's feature flags (preserve_order).
                let sorted: BTreeMap<String, Value> = map
                    .into_iter()
                    .map(|(key, value)| (key, canonical(value)))
                    .collect();
                Value::Object(sorted.into_iter().collect())
            }
            Value::Array(items) => Value::Array(items.into_iter().map(canonical).collect()),
            other => other,
        }
    }
    let value: Value = serde_json::from_slice(bytes)?;
    Ok(serde_json::to_vec(&canonical(value))?)
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

/// Sign a v3 release index together with a monotonic generation envelope.
///
/// The v3 payload itself is unchanged. Consumers that opt into key lifecycle
/// metadata verify this envelope and enforce its generation floor.
pub fn sign_release_index_with_generation(
    payload: ReleaseIndexPayload,
    release_generation: u64,
    signing_key: &SigningKey,
) -> Result<SignedReleaseIndexWithGenerationV1, ReleaseIndexError> {
    let index = sign_release_index(payload, signing_key)?;
    let mut envelope = SignedReleaseIndexWithGenerationV1 {
        schema_version: rill_runtime_protocol::RELEASE_INDEX_LIFECYCLE_SCHEMA_VERSION,
        release_generation,
        index,
        lifecycle_signature: String::new(),
    };
    let canonical = lifecycle_signing_bytes(&envelope)?;
    envelope.lifecycle_signature = hex::encode(signing_key.sign(&canonical).to_bytes());
    Ok(envelope)
}

/// Verify a signed v3 index against rotating trust metadata.
///
/// This is a new opt-in reader. The legacy `verify_release_index` path remains
/// exactly the v3 reader used by existing 1.x consumers. Trust metadata is
/// fail-closed: malformed, future, expired, revoked, emergency-revoked,
/// unknown-key and downgraded generations are rejected.
pub fn verify_release_index_with_trust_metadata(
    envelope: &SignedReleaseIndexWithGenerationV1,
    metadata: &TrustMetadataV1,
    floor: &rill_runtime_protocol::TrustVerificationFloorV1,
    now_unix_ms: u64,
) -> Result<(), ReleaseIndexError> {
    envelope
        .validate_shape()
        .map_err(|error| ReleaseIndexError::TrustMetadata(error.into()))?;
    metadata
        .validate_shape()
        .map_err(|error| ReleaseIndexError::TrustMetadata(error.into()))?;
    if metadata.metadata_generation < floor.minimum_metadata_generation {
        return Err(ReleaseIndexError::MetadataDowngrade);
    }
    if metadata.metadata_generation == floor.minimum_metadata_generation {
        let expected = floor
            .metadata_digest
            .as_deref()
            .ok_or(ReleaseIndexError::MetadataConflict)?;
        if !is_sha256_hex(expected) || trust_metadata_digest(metadata)? != expected {
            return Err(ReleaseIndexError::MetadataConflict);
        }
    }
    if envelope.release_generation < floor.minimum_release_generation
        || envelope.release_generation < metadata.minimum_release_generation
    {
        return Err(ReleaseIndexError::Downgrade);
    }
    let active = metadata
        .active_keys_at(now_unix_ms)
        .map_err(|error| ReleaseIndexError::TrustMetadata(error.into()))?;
    let mut keys = BTreeMap::new();
    for key in active {
        let bytes = hex::decode(&key.public_key_hex)
            .map_err(|_| ReleaseIndexError::TrustMetadata("invalid trust public key".into()))?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| ReleaseIndexError::TrustMetadata("invalid trust public key".into()))?;
        let public_key = VerifyingKey::from_bytes(&bytes)
            .map_err(|_| ReleaseIndexError::TrustMetadata("invalid trust public key".into()))?;
        keys.insert(key.key_id.clone(), public_key);
    }
    let publisher_key = keys
        .get(&envelope.index.payload.publisher_key_id)
        .ok_or(ReleaseIndexError::UnknownKey)?;
    let lifecycle_signature = hex::decode(&envelope.lifecycle_signature)
        .map_err(|_| ReleaseIndexError::TrustMetadata("invalid lifecycle signature".into()))?;
    let lifecycle_signature = Signature::from_slice(&lifecycle_signature)
        .map_err(|_| ReleaseIndexError::TrustMetadata("invalid lifecycle signature".into()))?;
    let canonical = lifecycle_signing_bytes(envelope)?;
    publisher_key
        .verify(&canonical, &lifecycle_signature)
        .map_err(|_| ReleaseIndexError::Signature)?;
    verify_release_index(&envelope.index, &TrustStore(keys))
}

/// Return the canonical SHA-256 identity a consumer may persist alongside a
/// metadata generation in [`rill_runtime_protocol::TrustVerificationFloorV1`].
pub fn trust_metadata_digest(metadata: &TrustMetadataV1) -> Result<String, ReleaseIndexError> {
    metadata
        .validate_shape()
        .map_err(|error| ReleaseIndexError::TrustMetadata(error.into()))?;
    let serialized = serde_json::to_vec(metadata)?;
    let canonical = canonical_json(&serialized).map_err(ReleaseIndexError::Canonical)?;
    Ok(hex::encode(Sha256::digest(canonical)))
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LifecycleSigningView<'a> {
    schema_version: u32,
    release_generation: u64,
    index: &'a rill_runtime_protocol::SignedReleaseIndex,
}

fn lifecycle_signing_bytes(
    envelope: &SignedReleaseIndexWithGenerationV1,
) -> Result<Vec<u8>, ReleaseIndexError> {
    let view = LifecycleSigningView {
        schema_version: envelope.schema_version,
        release_generation: envelope.release_generation,
        index: &envelope.index,
    };
    let serialized = serde_json::to_vec(&view)?;
    canonical_json(&serialized).map_err(ReleaseIndexError::Canonical)
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
    mut reader: R,
    allowed: &[&str],
    limits: ArchiveLimits,
) -> Result<BTreeMap<String, Vec<u8>>, ArchiveError> {
    let physical_count = validate_raw_central_directory(&mut reader, limits.max_files)?;
    let mut archive = ZipArchive::new(reader)?;
    if archive.len() != physical_count {
        return Err(ArchiveError::Duplicate(
            "ZIP names collapse to duplicate entries".into(),
        ));
    }
    if archive.len() > limits.max_files {
        return Err(ArchiveError::Limit("file count"));
    }
    let mut total = 0u64;
    let mut compressed_total = 0u64;
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
        if files.contains_key(&name) {
            return Err(ArchiveError::Duplicate(name));
        }
        if entry.size() > limits.max_file_bytes {
            return Err(ArchiveError::Limit("file size"));
        }
        let compressed = entry.compressed_size();
        // Use checked multiplication instead of integer division so the
        // comparison is exact: ``size / compressed`` truncates and would
        // accept an entry whose true ratio is just above the limit
        // (e.g. size=10, compressed=3, limit=3 → 10/3=3, accepted even
        // though 10 > 3*3). ``size > compressed * ratio`` avoids both the
        // truncation and any floating-point rounding, and the checked
        // product guards against u64 overflow on adversarial inputs.
        if compressed > 0 {
            let cap = compressed
                .checked_mul(limits.max_compression_ratio)
                .ok_or(ArchiveError::Limit("compression ratio"))?;
            if entry.size() > cap {
                return Err(ArchiveError::Limit("compression ratio"));
            }
        }
        let total_remaining = limits
            .max_total_bytes
            .checked_sub(total)
            .ok_or(ArchiveError::Limit("total size"))?;
        if entry.size() > total_remaining {
            return Err(ArchiveError::Limit("total size"));
        }
        compressed_total = compressed_total
            .checked_add(compressed)
            .ok_or(ArchiveError::Limit("compressed total size"))?;
        if compressed_total > limits.max_compressed_total_bytes {
            return Err(ArchiveError::Limit("compressed total size"));
        }
        let ratio_budget = compressed
            .checked_mul(limits.max_compression_ratio)
            .ok_or(ArchiveError::Limit("compression ratio"))?;
        let read_budget = limits.max_file_bytes.min(total_remaining).min(ratio_budget);
        let read_limit = read_budget
            .checked_add(1)
            .ok_or(ArchiveError::Limit("file size"))?;
        let mut bytes = Vec::new();
        entry.by_ref().take(read_limit).read_to_end(&mut bytes)?;
        let actual_size =
            u64::try_from(bytes.len()).map_err(|_| ArchiveError::Limit("file size"))?;
        if actual_size > read_budget {
            if actual_size > limits.max_file_bytes {
                return Err(ArchiveError::Limit("file size"));
            }
            if actual_size > total_remaining {
                return Err(ArchiveError::Limit("total size"));
            }
            if actual_size > ratio_budget {
                return Err(ArchiveError::Limit("compression ratio"));
            }
            return Err(ArchiveError::Limit("actual file size"));
        }
        if actual_size != entry.size() {
            return Err(ArchiveError::SizeMismatch(name));
        }
        total = total
            .checked_add(actual_size)
            .ok_or(ArchiveError::Limit("total size"))?;
        files.insert(name, bytes);
    }
    Ok(files)
}

/// Check the physical central-directory records before `zip` builds its
/// name-indexed map. The zip crate intentionally keeps only one entry for a
/// repeated name, which otherwise hides duplicates and evades the file cap.
fn validate_raw_central_directory<R: Read + Seek>(
    reader: &mut R,
    max_files: usize,
) -> Result<usize, ArchiveError> {
    const EOCD: u32 = 0x0605_4b50;
    const CENTRAL: u32 = 0x0201_4b50;
    const MAX_TAIL: usize = 22 + u16::MAX as usize;

    let archive_end = reader.seek(std::io::SeekFrom::End(0))?;
    let tail_len = archive_end.min(MAX_TAIL as u64) as usize;
    if tail_len < 22 {
        return Err(
            std::io::Error::new(std::io::ErrorKind::InvalidData, "missing ZIP end record").into(),
        );
    }
    reader.seek(std::io::SeekFrom::End(-(tail_len as i64)))?;
    let mut tail = vec![0; tail_len];
    reader.read_exact(&mut tail)?;
    let eocd_index = (0..=tail.len().saturating_sub(22))
        .rev()
        .find(|&i| {
            tail.get(i..i + 4)
                .and_then(|v| v.try_into().ok())
                .map(u32::from_le_bytes)
                == Some(EOCD)
                && i + 22 + u16::from_le_bytes([tail[i + 20], tail[i + 21]]) as usize == tail.len()
        })
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "missing ZIP end record")
        })?;
    let eocd = &tail[eocd_index..];
    let disk = u16::from_le_bytes([eocd[4], eocd[5]]);
    let central_disk = u16::from_le_bytes([eocd[6], eocd[7]]);
    let disk_count = u16::from_le_bytes([eocd[8], eocd[9]]);
    let count = u16::from_le_bytes([eocd[10], eocd[11]]) as usize;
    let central_size = u32::from_le_bytes(eocd[12..16].try_into().unwrap()) as u64;
    if disk != 0 || central_disk != 0 || disk_count as usize != count {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "multi-disk ZIP is unsupported",
        )
        .into());
    }
    if count > max_files {
        return Err(ArchiveError::Limit("file count"));
    }
    // These package formats are deliberately tiny; ZIP64 count sentinels
    // necessarily exceed their configured entry limits.
    if count == u16::MAX as usize || central_size == u32::MAX as u64 {
        return Err(ArchiveError::Limit("file count"));
    }
    let eocd_absolute = archive_end - (tail.len() - eocd_index) as u64;
    let central_start = eocd_absolute.checked_sub(central_size).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid ZIP central directory bounds",
        )
    })?;
    reader.seek(std::io::SeekFrom::Start(central_start))?;
    let mut names = BTreeSet::new();
    let mut consumed = 0u64;
    for _ in 0..count {
        let mut header = [0u8; 46];
        reader.read_exact(&mut header)?;
        if u32::from_le_bytes(header[0..4].try_into().unwrap()) != CENTRAL {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid ZIP central directory record",
            )
            .into());
        }
        let name_len = u16::from_le_bytes(header[28..30].try_into().unwrap()) as usize;
        let extra_len = u16::from_le_bytes(header[30..32].try_into().unwrap()) as usize;
        let comment_len = u16::from_le_bytes(header[32..34].try_into().unwrap()) as usize;
        let mut name = vec![0; name_len];
        reader.read_exact(&mut name)?;
        if !names.insert(name) {
            return Err(ArchiveError::Duplicate("duplicate raw ZIP entry".into()));
        }
        let skip = extra_len
            .checked_add(comment_len)
            .ok_or(ArchiveError::Limit("file count"))?;
        reader.seek(std::io::SeekFrom::Current(skip as i64))?;
        consumed = consumed
            .checked_add(46 + name_len as u64 + skip as u64)
            .ok_or(ArchiveError::Limit("file count"))?;
    }
    if consumed != central_size {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "ZIP central directory size mismatch",
        )
        .into());
    }
    Ok(count)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// CRC-32 of `data` (matching the value stored in each ZIP local header
    /// and central-directory record).
    fn crc32(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFFFFFF;
        for &byte in data {
            crc ^= byte as u32;
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xEDB88320 & (0u32.wrapping_sub(crc & 1)));
            }
        }
        !crc
    }

    /// Build a minimal stored (uncompressed) ZIP archive whose single entry
    /// reports `uncompressed_size` and `compressed_size` independently in
    /// both the local file header and the central directory.
    ///
    /// The zip crate's `ZipWriter` always sets both fields to `data.len()`,
    /// which makes it impossible to exercise the compression-ratio check.
    /// Writing the bytes by hand lets the tests pretend the entry compressed
    /// to a different size than its payload.
    fn build_zip_with_sizes(
        name: &str,
        data: &[u8],
        uncompressed_size: u32,
        compressed_size: u32,
    ) -> Vec<u8> {
        let crc = crc32(data);
        let mut buf = Vec::new();
        let local_offset = 0u32;

        // Local file header.
        buf.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
        buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
        buf.extend_from_slice(&0u16.to_le_bytes()); // flags
        buf.extend_from_slice(&0u16.to_le_bytes()); // method = stored
        buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
        buf.extend_from_slice(&0u16.to_le_bytes()); // mod date
        buf.extend_from_slice(&crc.to_le_bytes());
        buf.extend_from_slice(&compressed_size.to_le_bytes());
        buf.extend_from_slice(&uncompressed_size.to_le_bytes());
        buf.extend_from_slice(&(name.len() as u16).to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // extra length
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(data);

        let cd_start = buf.len() as u32;

        // Central directory file header.
        buf.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
        buf.extend_from_slice(&20u16.to_le_bytes()); // version made by
        buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
        buf.extend_from_slice(&0u16.to_le_bytes()); // flags
        buf.extend_from_slice(&0u16.to_le_bytes()); // method
        buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
        buf.extend_from_slice(&0u16.to_le_bytes()); // mod date
        buf.extend_from_slice(&crc.to_le_bytes());
        buf.extend_from_slice(&compressed_size.to_le_bytes());
        buf.extend_from_slice(&uncompressed_size.to_le_bytes());
        buf.extend_from_slice(&(name.len() as u16).to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // extra length
        buf.extend_from_slice(&0u16.to_le_bytes()); // comment length
        buf.extend_from_slice(&0u16.to_le_bytes()); // disk number
        buf.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        buf.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        buf.extend_from_slice(&local_offset.to_le_bytes());
        buf.extend_from_slice(name.as_bytes());

        let cd_size = buf.len() as u32 - cd_start;

        // End of central directory record.
        buf.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
        buf.extend_from_slice(&0u16.to_le_bytes()); // disk number
        buf.extend_from_slice(&0u16.to_le_bytes()); // disk with CD
        buf.extend_from_slice(&1u16.to_le_bytes()); // entries on this disk
        buf.extend_from_slice(&1u16.to_le_bytes()); // total entries
        buf.extend_from_slice(&cd_size.to_le_bytes());
        buf.extend_from_slice(&cd_start.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // comment length

        buf
    }

    fn limits_with_ratio(ratio: u64) -> ArchiveLimits {
        ArchiveLimits {
            max_files: 10,
            max_file_bytes: 1024 * 1024,
            max_total_bytes: 1024 * 1024,
            max_compressed_total_bytes: 1024 * 1024,
            max_compression_ratio: ratio,
        }
    }

    fn raw_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut output = Vec::new();
        let mut offsets = Vec::new();
        for (name, data) in entries {
            offsets.push(output.len() as u32);
            let crc = crc32(data);
            output.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            output.extend_from_slice(&20u16.to_le_bytes());
            output.extend_from_slice(&0u16.to_le_bytes());
            output.extend_from_slice(&0u16.to_le_bytes());
            output.extend_from_slice(&0u16.to_le_bytes());
            output.extend_from_slice(&0u16.to_le_bytes());
            output.extend_from_slice(&crc.to_le_bytes());
            output.extend_from_slice(&(data.len() as u32).to_le_bytes());
            output.extend_from_slice(&(data.len() as u32).to_le_bytes());
            output.extend_from_slice(&(name.len() as u16).to_le_bytes());
            output.extend_from_slice(&0u16.to_le_bytes());
            output.extend_from_slice(name.as_bytes());
            output.extend_from_slice(data);
        }
        let central_start = output.len() as u32;
        for ((name, data), offset) in entries.iter().zip(offsets) {
            let crc = crc32(data);
            output.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            output.extend_from_slice(&20u16.to_le_bytes());
            output.extend_from_slice(&20u16.to_le_bytes());
            output.extend_from_slice(&0u16.to_le_bytes());
            output.extend_from_slice(&0u16.to_le_bytes());
            output.extend_from_slice(&0u16.to_le_bytes());
            output.extend_from_slice(&0u16.to_le_bytes());
            output.extend_from_slice(&crc.to_le_bytes());
            output.extend_from_slice(&(data.len() as u32).to_le_bytes());
            output.extend_from_slice(&(data.len() as u32).to_le_bytes());
            output.extend_from_slice(&(name.len() as u16).to_le_bytes());
            output.extend_from_slice(&0u16.to_le_bytes());
            output.extend_from_slice(&0u16.to_le_bytes());
            output.extend_from_slice(&0u16.to_le_bytes());
            output.extend_from_slice(&0u16.to_le_bytes());
            output.extend_from_slice(&0u32.to_le_bytes());
            output.extend_from_slice(&offset.to_le_bytes());
            output.extend_from_slice(name.as_bytes());
        }
        let central_size = output.len() as u32 - central_start;
        output.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        output.extend_from_slice(&0u16.to_le_bytes());
        output.extend_from_slice(&0u16.to_le_bytes());
        output.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        output.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        output.extend_from_slice(&central_size.to_le_bytes());
        output.extend_from_slice(&central_start.to_le_bytes());
        output.extend_from_slice(&0u16.to_le_bytes());
        output
    }

    #[test]
    fn rejects_duplicate_physical_entries_before_zip_indexing() {
        let bytes = raw_zip(&[("manifest.json", b"one"), ("manifest.json", b"two")]);
        let error = read_archive(
            Cursor::new(bytes),
            &["manifest.json"],
            limits_with_ratio(100),
        )
        .unwrap_err();
        assert!(matches!(error, ArchiveError::Duplicate(_)));
    }

    #[test]
    fn file_limit_counts_physical_entries_before_zip_indexing() {
        let bytes = raw_zip(&[("a", b"one"), ("a", b"two"), ("b", b"three")]);
        let mut limits = limits_with_ratio(100);
        limits.max_files = 2;
        let error = read_archive(Cursor::new(bytes), &["a", "b"], limits).unwrap_err();
        assert!(matches!(error, ArchiveError::Limit("file count")));
    }

    #[test]
    fn truncated_zip_end_record_is_rejected_without_panicking() {
        let error = read_archive(
            Cursor::new(b"PK\x05\x06"),
            &["manifest.json"],
            limits_with_ratio(100),
        )
        .unwrap_err();
        assert!(matches!(error, ArchiveError::Io(_)));
    }

    fn build_deflated_zip_with_declared_size(
        name: &str,
        data: &[u8],
        declared_size: u32,
    ) -> Vec<u8> {
        build_deflated_zip_with_declared_sizes(&[(name, data, declared_size)])
    }

    fn build_deflated_zip_with_declared_sizes(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        {
            let mut archive = ZipWriter::new(&mut output);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            for (name, data, _) in entries {
                archive.start_file(*name, options).unwrap();
                archive.write_all(data).unwrap();
            }
            archive.finish().unwrap();
        }
        let mut bytes = output.into_inner();
        let mut local_offset = 0;
        for (_, _, declared_size) in entries {
            assert_eq!(
                &bytes[local_offset..local_offset + 4],
                &[0x50, 0x4b, 0x03, 0x04]
            );
            let compressed_size = u32::from_le_bytes(
                bytes[local_offset + 18..local_offset + 22]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let name_len = u16::from_le_bytes(
                bytes[local_offset + 26..local_offset + 28]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let extra_len = u16::from_le_bytes(
                bytes[local_offset + 28..local_offset + 30]
                    .try_into()
                    .unwrap(),
            ) as usize;
            bytes[local_offset + 22..local_offset + 26]
                .copy_from_slice(&declared_size.to_le_bytes());
            local_offset += 30 + name_len + extra_len + compressed_size;
        }
        let mut central_offset = local_offset;
        for (_, _, declared_size) in entries {
            assert_eq!(
                &bytes[central_offset..central_offset + 4],
                &[0x50, 0x4b, 0x01, 0x02]
            );
            let name_len = u16::from_le_bytes(
                bytes[central_offset + 28..central_offset + 30]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let extra_len = u16::from_le_bytes(
                bytes[central_offset + 30..central_offset + 32]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let comment_len = u16::from_le_bytes(
                bytes[central_offset + 32..central_offset + 34]
                    .try_into()
                    .unwrap(),
            ) as usize;
            bytes[central_offset + 24..central_offset + 28]
                .copy_from_slice(&declared_size.to_le_bytes());
            central_offset += 46 + name_len + extra_len + comment_len;
        }
        bytes
    }

    #[test]
    fn rejects_deflate_stream_that_exceeds_declared_size_before_accepting_contents() {
        let payload = vec![b'X'; 1024 * 1024];
        let zip = build_deflated_zip_with_declared_size("payload.bin", &payload, 1);
        let mut limits = limits_with_ratio(1024);
        limits.max_file_bytes = 1024;
        limits.max_total_bytes = 1024;

        let result = read_archive(std::io::Cursor::new(zip), &["payload.bin"], limits);
        assert!(
            matches!(result, Err(ArchiveError::Limit("file size"))),
            "under-declared DEFLATE stream must be rejected by its actual bytes"
        );
    }

    #[test]
    fn rejects_actual_cumulative_total_and_size_mismatch() {
        let first = [b'A'; 10];
        let second = [b'B'; 10];
        let zip = build_deflated_zip_with_declared_sizes(&[
            ("first.bin", &first, 10),
            ("second.bin", &second, 1),
        ]);
        let mut limits = limits_with_ratio(1000);
        limits.max_file_bytes = 10;
        limits.max_total_bytes = 15;
        let result = read_archive(Cursor::new(zip), &["first.bin", "second.bin"], limits);
        assert!(
            matches!(result, Err(ArchiveError::Limit("total size"))),
            "expected actual total rejection, got: {:?}",
            result.as_ref().err()
        );

        let zip = build_deflated_zip_with_declared_size("payload.bin", &first, 9);
        let mut limits = limits_with_ratio(1000);
        limits.max_file_bytes = 10;
        limits.max_total_bytes = 10;
        let result = read_archive(Cursor::new(zip), &["payload.bin"], limits);
        assert!(matches!(result, Err(ArchiveError::SizeMismatch(_))));
    }

    #[test]
    fn enforces_exact_file_limit_and_rejects_bad_crc_and_compression_method() {
        let data = b"0123456789";
        let zip = build_zip_with_sizes("payload.bin", data, data.len() as u32, data.len() as u32);
        let mut limits = limits_with_ratio(1);
        limits.max_file_bytes = data.len() as u64;
        limits.max_total_bytes = data.len() as u64;
        assert!(read_archive(Cursor::new(&zip), &["payload.bin"], limits).is_ok());

        let too_large = build_zip_with_sizes("payload.bin", data, 11, 10);
        assert!(matches!(
            read_archive(Cursor::new(&too_large), &["payload.bin"], limits),
            Err(ArchiveError::Limit("file size"))
        ));

        let mut bad_crc = zip.clone();
        let crc = crc32(data) ^ 1;
        bad_crc[14..18].copy_from_slice(&crc.to_le_bytes());
        let central_offset = bad_crc
            .windows(4)
            .rposition(|window| window == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        bad_crc[central_offset + 16..central_offset + 20].copy_from_slice(&crc.to_le_bytes());
        let bad_crc_result = read_archive(Cursor::new(&bad_crc), &["payload.bin"], limits);
        assert!(
            matches!(&bad_crc_result, Err(ArchiveError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData),
            "bad CRC must propagate from ZIP stream verification; got: {:?}",
            bad_crc_result.as_ref().err()
        );

        let mut unsupported_method = zip;
        unsupported_method[8..10].copy_from_slice(&99u16.to_le_bytes());
        let central_offset = unsupported_method
            .windows(4)
            .rposition(|window| window == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        unsupported_method[central_offset + 10..central_offset + 12]
            .copy_from_slice(&99u16.to_le_bytes());
        assert!(matches!(
            read_archive(Cursor::new(&unsupported_method), &["payload.bin"], limits),
            Err(ArchiveError::Zip(_))
        ));
    }

    #[test]
    fn compression_ratio_accepts_exact_boundary() {
        // size = compressed * ratio exactly. The previous integer-division
        // implementation accepted this case, and the new checked-multiplication
        // implementation must continue to accept it so the limit remains the
        // boundary, not `ratio - 1`.
        //
        // For stored (uncompressed) entries the zip crate reads
        // `compressed_size` bytes from the local header, so the data buffer
        // must be exactly that long. `uncompressed_size` is reported
        // independently by `entry.size()` and is what the ratio check uses.
        let data = b"0123456789"; // 10 bytes, stored ZIP member (ratio 1)
        let zip = build_zip_with_sizes("payload.bin", data, 10, 10);
        let files = read_archive(
            std::io::Cursor::new(&zip),
            &["payload.bin"],
            limits_with_ratio(1),
        )
        .expect("exact boundary must be accepted");
        assert_eq!(files.get("payload.bin").map(Vec::as_slice), Some(&data[..]));
    }

    #[test]
    fn compression_ratio_rejects_one_byte_over_boundary() {
        // Regression for the integer-division truncation bug: with the old
        // `size / compressed > ratio` check, size=1001/compressed=10/ratio=100
        // evaluated to `100 > 100` = false and was accepted even though the
        // true ratio is 100.1. The new check must reject it.
        let data = b"0123456789"; // 10 bytes
        let zip = build_zip_with_sizes("payload.bin", data, 1001, 10);
        let result = read_archive(
            std::io::Cursor::new(&zip),
            &["payload.bin"],
            limits_with_ratio(100),
        );
        assert!(
            matches!(result, Err(ArchiveError::Limit("compression ratio"))),
            "expected compression-ratio rejection, got: {result:?}"
        );
    }

    #[test]
    fn compression_ratio_skips_zero_compressed_size() {
        // A zero compressed_size must not divide by zero or trigger the
        // ratio check. The entry is accepted (the size limit still applies).
        let zip = build_zip_with_sizes("payload.bin", b"", 0, 0);
        let files = read_archive(
            std::io::Cursor::new(&zip),
            &["payload.bin"],
            limits_with_ratio(100),
        )
        .expect("zero-size entry must be accepted");
        assert!(files.get("payload.bin").map(Vec::is_empty).unwrap_or(false));
    }

    #[test]
    fn compression_ratio_rejects_overflowing_product() {
        // Adversarial compressed_size * ratio that overflows u64 must be
        // rejected via checked_mul rather than wrapping around to a small
        // value that would let the attack through.
        //
        // compressed_size = 2 (data buffer is 2 bytes), ratio = u64::MAX.
        // 2 * u64::MAX overflows u64; without checked_mul the wrapping
        // product would be u64::MAX - 1, and `entry.size() > u64::MAX - 1`
        // would be false for any small size, letting the attack through.
        let data = b"xy"; // 2 bytes
        let zip = build_zip_with_sizes("payload.bin", data, 2, 2);
        let limits = ArchiveLimits {
            max_files: 10,
            max_file_bytes: 1024 * 1024,
            max_total_bytes: 1024 * 1024,
            max_compressed_total_bytes: 1024 * 1024,
            max_compression_ratio: u64::MAX,
        };
        let result = read_archive(std::io::Cursor::new(&zip), &["payload.bin"], limits);
        assert!(
            matches!(result, Err(ArchiveError::Limit("compression ratio"))),
            "expected overflow rejection, got: {result:?}"
        );
    }

    fn lifecycle_payload() -> ReleaseIndexPayload {
        use rill_runtime_protocol::{
            ReleaseArtifact, ReleaseArtifactKind, RELEASE_INDEX_SCHEMA_VERSION,
            RUNTIME_API_VERSION, RUNTIME_ARTIFACT_ID,
        };
        ReleaseIndexPayload {
            schema_version: RELEASE_INDEX_SCHEMA_VERSION,
            channel: "stable".into(),
            generated_at: "2026-08-22T00:00:00Z".into(),
            publisher_key_id: "current".into(),
            artifacts: vec![ReleaseArtifact {
                kind: ReleaseArtifactKind::Runtime,
                id: RUNTIME_ARTIFACT_ID.into(),
                version: "1.3.0".into(),
                runtime_api_version: RUNTIME_API_VERSION,
                target_os: Some("linux".into()),
                target_arch: Some("x86_64".into()),
                target_libc: Some("gnu".into()),
                handler_api_version: None,
                min_runtime_version: None,
                pm_adapter_protocol_version: None,
                url: "https://example.invalid/runtime".into(),
                sha256: "ab".repeat(32),
                size: 1024,
            }],
        }
    }

    fn metadata(signing: &SigningKey) -> rill_runtime_protocol::TrustMetadataV1 {
        use rill_runtime_protocol::{
            TrustKeyMetadataV1, TrustKeyRole, TRUST_METADATA_SCHEMA_VERSION,
        };
        rill_runtime_protocol::TrustMetadataV1 {
            schema_version: TRUST_METADATA_SCHEMA_VERSION,
            metadata_generation: 1,
            minimum_release_generation: 1,
            keys: vec![TrustKeyMetadataV1 {
                key_id: "current".into(),
                public_key_hex: hex::encode(signing.verifying_key().to_bytes()),
                role: TrustKeyRole::Current,
                not_before_unix_ms: 0,
                not_after_unix_ms: None,
                revoked_at_unix_ms: None,
                emergency_revoked: false,
            }],
        }
    }

    fn floor(
        metadata: &rill_runtime_protocol::TrustMetadataV1,
    ) -> rill_runtime_protocol::TrustVerificationFloorV1 {
        rill_runtime_protocol::TrustVerificationFloorV1 {
            minimum_metadata_generation: metadata.metadata_generation,
            minimum_release_generation: metadata.minimum_release_generation,
            metadata_digest: Some(trust_metadata_digest(metadata).unwrap()),
        }
    }

    #[test]
    fn trust_metadata_current_key_passes() {
        let signing = SigningKey::from_bytes(&[71; 32]);
        let envelope =
            sign_release_index_with_generation(lifecycle_payload(), 1, &signing).unwrap();
        let trust = metadata(&signing);
        verify_release_index_with_trust_metadata(&envelope, &trust, &floor(&trust), 100).unwrap();
    }

    #[test]
    fn trust_metadata_overlap_next_key_passes() {
        use rill_runtime_protocol::{TrustKeyMetadataV1, TrustKeyRole};
        let current = SigningKey::from_bytes(&[72; 32]);
        let next = SigningKey::from_bytes(&[73; 32]);
        let mut trust = metadata(&current);
        trust.keys.push(TrustKeyMetadataV1 {
            key_id: "next".into(),
            public_key_hex: hex::encode(next.verifying_key().to_bytes()),
            role: TrustKeyRole::Next,
            not_before_unix_ms: 50,
            not_after_unix_ms: None,
            revoked_at_unix_ms: None,
            emergency_revoked: false,
        });
        let mut payload = lifecycle_payload();
        payload.publisher_key_id = "next".into();
        let envelope = sign_release_index_with_generation(payload, 2, &next).unwrap();
        verify_release_index_with_trust_metadata(&envelope, &trust, &floor(&trust), 100).unwrap();
    }

    #[test]
    fn trust_metadata_rejects_future_expired_revoked_and_emergency_keys() {
        let signing = SigningKey::from_bytes(&[74; 32]);
        let envelope =
            sign_release_index_with_generation(lifecycle_payload(), 1, &signing).unwrap();
        let mut future = metadata(&signing);
        future.keys[0].not_before_unix_ms = 101;
        let future_floor = floor(&future);
        assert!(
            verify_release_index_with_trust_metadata(&envelope, &future, &future_floor, 100)
                .is_err()
        );
        let mut expired = metadata(&signing);
        expired.keys[0].not_after_unix_ms = Some(100);
        let expired_floor = floor(&expired);
        assert!(
            verify_release_index_with_trust_metadata(&envelope, &expired, &expired_floor, 100)
                .is_err()
        );
        let mut revoked = metadata(&signing);
        revoked.keys[0].revoked_at_unix_ms = Some(100);
        let revoked_floor = floor(&revoked);
        assert!(
            verify_release_index_with_trust_metadata(&envelope, &revoked, &revoked_floor, 100)
                .is_err()
        );
        let mut emergency = metadata(&signing);
        emergency.keys[0].emergency_revoked = true;
        let emergency_floor = floor(&emergency);
        assert!(verify_release_index_with_trust_metadata(
            &envelope,
            &emergency,
            &emergency_floor,
            100
        )
        .is_err());
    }

    #[test]
    fn trust_metadata_rejects_unknown_damaged_and_downgraded() {
        use rill_runtime_protocol::TrustKeyMetadataV1;
        let signing = SigningKey::from_bytes(&[75; 32]);
        let envelope =
            sign_release_index_with_generation(lifecycle_payload(), 1, &signing).unwrap();
        let other = SigningKey::from_bytes(&[76; 32]);
        let mut unknown = metadata(&other);
        unknown.keys[0].key_id = "other".into();
        assert!(matches!(
            verify_release_index_with_trust_metadata(&envelope, &unknown, &floor(&unknown), 100),
            Err(ReleaseIndexError::UnknownKey)
        ));
        let accepted = metadata(&signing);
        let accepted_floor = floor(&accepted);
        let mut damaged = metadata(&signing);
        damaged.keys.push(TrustKeyMetadataV1 {
            key_id: "current".into(),
            public_key_hex: hex::encode(signing.verifying_key().to_bytes()),
            role: rill_runtime_protocol::TrustKeyRole::Next,
            not_before_unix_ms: 0,
            not_after_unix_ms: None,
            revoked_at_unix_ms: None,
            emergency_revoked: false,
        });
        assert!(matches!(
            verify_release_index_with_trust_metadata(&envelope, &damaged, &accepted_floor, 100),
            Err(ReleaseIndexError::TrustMetadata(_))
        ));
        let mut downgrade = metadata(&signing);
        downgrade.minimum_release_generation = 2;
        assert!(matches!(
            verify_release_index_with_trust_metadata(
                &envelope,
                &downgrade,
                &floor(&downgrade),
                100
            ),
            Err(ReleaseIndexError::Downgrade)
        ));
    }

    #[test]
    fn trust_metadata_rejects_generation_tampering() {
        let signing = SigningKey::from_bytes(&[77; 32]);
        let mut envelope =
            sign_release_index_with_generation(lifecycle_payload(), 1, &signing).unwrap();
        envelope.release_generation = 2;
        let trust = metadata(&signing);
        assert!(matches!(
            verify_release_index_with_trust_metadata(&envelope, &trust, &floor(&trust), 100),
            Err(ReleaseIndexError::Signature)
        ));
    }

    #[test]
    fn trust_metadata_rejects_old_authenticated_document_after_revocation() {
        use rill_runtime_protocol::{TrustKeyMetadataV1, TrustKeyRole};
        let current = SigningKey::from_bytes(&[78; 32]);
        let revoked = SigningKey::from_bytes(&[79; 32]);
        let mut accepted = metadata(&current);
        accepted.metadata_generation = 10;
        accepted.keys.push(TrustKeyMetadataV1 {
            key_id: "revoked".into(),
            public_key_hex: hex::encode(revoked.verifying_key().to_bytes()),
            role: TrustKeyRole::Next,
            not_before_unix_ms: 0,
            not_after_unix_ms: None,
            revoked_at_unix_ms: Some(100),
            emergency_revoked: false,
        });
        let consumer_floor = floor(&accepted);

        let mut old = accepted.clone();
        old.metadata_generation = 9;
        old.keys[1].revoked_at_unix_ms = None;
        let mut payload = lifecycle_payload();
        payload.publisher_key_id = "revoked".into();
        let envelope = sign_release_index_with_generation(payload, 2, &revoked).unwrap();

        assert!(matches!(
            verify_release_index_with_trust_metadata(&envelope, &old, &consumer_floor, 100),
            Err(ReleaseIndexError::MetadataDowngrade)
        ));
    }

    #[test]
    fn trust_metadata_rejects_same_generation_content_conflict() {
        let signing = SigningKey::from_bytes(&[80; 32]);
        let accepted = metadata(&signing);
        let floor = floor(&accepted);
        let mut conflicting = accepted.clone();
        conflicting.keys[0].emergency_revoked = true;
        let envelope =
            sign_release_index_with_generation(lifecycle_payload(), 1, &signing).unwrap();

        assert!(matches!(
            verify_release_index_with_trust_metadata(&envelope, &conflicting, &floor, 100),
            Err(ReleaseIndexError::MetadataConflict)
        ));
    }

    #[test]
    fn trust_metadata_accepts_future_generation_for_consumer_promotion() {
        let signing = SigningKey::from_bytes(&[81; 32]);
        let accepted = metadata(&signing);
        let floor = floor(&accepted);
        let mut future = accepted.clone();
        future.metadata_generation = accepted.metadata_generation + 1;
        let envelope =
            sign_release_index_with_generation(lifecycle_payload(), 2, &signing).unwrap();

        verify_release_index_with_trust_metadata(&envelope, &future, &floor, 100).unwrap();
    }
}
