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
    max_compressed_total_bytes: 8 * 1024 * 1024,
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

    use std::io::{Cursor, Write};
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn build_malicious_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut output);
            let options = SimpleFileOptions::default();
            for (name, data) in files {
                zip.start_file(name, options).unwrap();
                zip.write_all(data).unwrap();
            }
            zip.finish().unwrap();
        }
        output.into_inner()
    }

    #[test]
    fn handler_pack_rejects_extra_file() {
        let files: &[(&str, &[u8])] = &[
            ("manifest.json", b"{}"),
            ("handler.wasm", b"wasm"),
            ("checksums.json", b"{}"),
            ("META-INF/signature.ed25519", b"sig"),
            ("evil.txt", b"evil"),
        ];
        let bytes = build_malicious_zip(files);
        let trust = TrustStore(BTreeMap::new());
        assert!(matches!(
            load_handler_pack(std::io::Cursor::new(&bytes), &trust),
            Err(HandlerPackError::Archive(ArchiveError::Forbidden(_)))
        ));
    }

    #[test]
    fn handler_pack_rejects_path_traversal() {
        let bytes = build_malicious_zip(&[("../escape.txt", b"escape")]);
        let trust = TrustStore(BTreeMap::new());
        assert!(matches!(
            load_handler_pack(std::io::Cursor::new(&bytes), &trust),
            Err(HandlerPackError::Archive(ArchiveError::UnsafePath(_)))
        ));
    }

    #[test]
    fn handler_pack_rejects_absolute_path() {
        let bytes = build_malicious_zip(&[("/etc/passwd", b"root")]);
        let trust = TrustStore(BTreeMap::new());
        assert!(matches!(
            load_handler_pack(std::io::Cursor::new(&bytes), &trust),
            Err(HandlerPackError::Archive(ArchiveError::UnsafePath(_)))
        ));
    }

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

    // Builds a ZIP from raw bytes so duplicate entry names can be emitted,
    // which the zip crate's ZipWriter refuses to create.
    fn build_raw_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut offsets: Vec<u32> = Vec::new();
        for (name, data) in files {
            let crc = crc32(data);
            offsets.push(buf.len() as u32);
            // Local file header
            buf.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
            buf.extend_from_slice(&20u16.to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes());
            buf.extend_from_slice(&crc.to_le_bytes());
            buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
            buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
            buf.extend_from_slice(&(name.len() as u16).to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes());
            buf.extend_from_slice(name.as_bytes());
            buf.extend_from_slice(data);
        }
        let cd_start = buf.len() as u32;
        for (i, (name, data)) in files.iter().enumerate() {
            let crc = crc32(data);
            // Central directory file header
            buf.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
            buf.extend_from_slice(&20u16.to_le_bytes());
            buf.extend_from_slice(&20u16.to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes());
            buf.extend_from_slice(&crc.to_le_bytes());
            buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
            buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
            buf.extend_from_slice(&(name.len() as u16).to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes());
            buf.extend_from_slice(&0u32.to_le_bytes());
            buf.extend_from_slice(&offsets[i].to_le_bytes());
            buf.extend_from_slice(name.as_bytes());
        }
        let cd_size = buf.len() as u32 - cd_start;
        // End of central directory record
        buf.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&(files.len() as u16).to_le_bytes());
        buf.extend_from_slice(&(files.len() as u16).to_le_bytes());
        buf.extend_from_slice(&cd_size.to_le_bytes());
        buf.extend_from_slice(&cd_start.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf
    }

    #[test]
    fn handler_pack_rejects_duplicate_entry() {
        // The zip crate deduplicates same-named entries in its IndexMap at
        // read time, so ArchiveError::Duplicate is defensive. Build a raw ZIP
        // with two manifest.json entries and verify the archive is rejected.
        let bytes = build_raw_zip(&[("manifest.json", b"first"), ("manifest.json", b"second")]);
        let trust = TrustStore(BTreeMap::new());
        assert!(load_handler_pack(std::io::Cursor::new(&bytes), &trust).is_err());
    }

    #[test]
    fn handler_pack_rejects_invalid_semver() {
        // validate_shape() only checks that `version` is non-empty and <=48
        // chars; the actual semver rejection happens in load_handler_pack via
        // semver::Version::parse, so exercise that path here.
        let signing = SigningKey::from_bytes(&[20; 32]);
        let key_id = "semver-test";
        let module = b"semver module";
        let mut bad_manifest = manifest(key_id, module);
        bad_manifest.version = "not-semver".into();
        let bytes = build_signed_handler_pack(&bad_manifest, module, &signing).unwrap();
        let trust = TrustStore(BTreeMap::from([(key_id.into(), signing.verifying_key())]));
        assert!(matches!(
            load_handler_pack(std::io::Cursor::new(&bytes), &trust),
            Err(HandlerPackError::Manifest(_))
        ));
    }

    #[test]
    fn handler_pack_rejects_duplicate_capabilities() {
        let module = b"dup cap module";
        let mut bad_manifest = manifest("dup-cap-test", module);
        bad_manifest.capabilities = vec!["dup".into(), "dup".into()];
        assert!(bad_manifest.validate_shape().is_err());
    }

    #[test]
    fn handler_pack_rejects_unknown_format_version() {
        let module = b"format module";
        let mut bad_manifest = manifest("format-test", module);
        bad_manifest.format_version = 99;
        assert!(bad_manifest.validate_shape().is_err());
    }

    #[test]
    fn handler_pack_rejects_unknown_handler_api_version() {
        let module = b"api module";
        let mut bad_manifest = manifest("api-test", module);
        bad_manifest.handler_api_version = 99;
        assert!(bad_manifest.validate_shape().is_err());
    }

    #[test]
    fn handler_pack_rejects_empty_capabilities() {
        let module = b"empty cap module";
        let mut bad_manifest = manifest("empty-cap-test", module);
        bad_manifest.capabilities = vec![];
        assert!(bad_manifest.validate_shape().is_err());
    }

    #[test]
    fn handler_pack_rejects_oversized_module() {
        let module = b"oversized module";
        let mut bad_manifest = manifest("oversized-test", module);
        bad_manifest.module_size = 5 * 1024 * 1024;
        assert!(bad_manifest.validate_shape().is_err());
    }

    // ----- R-019: handler package attack tests -----

    use std::io::Read;

    /// Read all (name → bytes) entries from a ZIP archive.
    fn read_zip_entries(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        let mut entries = BTreeMap::new();
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).unwrap();
            let name = file.name().to_string();
            let mut data = Vec::new();
            file.read_to_end(&mut data).unwrap();
            entries.insert(name, data);
        }
        entries
    }

    /// Rebuild a signed pack with one file's bytes tampered at the midpoint.
    fn rebuild_with_tamper(bytes: &[u8], name_to_tamper: &str) -> Vec<u8> {
        let mut entries = read_zip_entries(bytes);
        let data = entries.get_mut(name_to_tamper).unwrap();
        let idx = data.len() / 2;
        data[idx] ^= 0xFF;
        let files: Vec<(&str, &[u8])> = entries
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_slice()))
            .collect();
        build_malicious_zip(&files)
    }

    #[test]
    fn handler_pack_rejects_tampered_manifest() {
        let signing = SigningKey::from_bytes(&[31; 32]);
        let key_id = "tamper-manifest";
        let module = b"tamper manifest module";
        let bytes = build_signed_handler_pack(&manifest(key_id, module), module, &signing).unwrap();
        let tampered = rebuild_with_tamper(&bytes, "manifest.json");
        let trust = TrustStore(BTreeMap::from([(key_id.into(), signing.verifying_key())]));
        // Tampering manifest bytes breaks either JSON parsing (→ Json error)
        // or the checksum digest (→ Digest error). Either rejection is correct.
        let result = load_handler_pack(std::io::Cursor::new(&tampered), &trust);
        assert!(result.is_err(), "tampered manifest must be rejected");
    }

    #[test]
    fn handler_pack_rejects_tampered_module() {
        let signing = SigningKey::from_bytes(&[32; 32]);
        let key_id = "tamper-module";
        let module = b"tamper module bytes";
        let bytes = build_signed_handler_pack(&manifest(key_id, module), module, &signing).unwrap();
        let tampered = rebuild_with_tamper(&bytes, "handler.wasm");
        let trust = TrustStore(BTreeMap::from([(key_id.into(), signing.verifying_key())]));
        assert!(matches!(
            load_handler_pack(std::io::Cursor::new(&tampered), &trust),
            Err(HandlerPackError::Archive(ArchiveError::Digest(_)))
        ));
    }

    #[test]
    fn handler_pack_rejects_tampered_checksums() {
        let signing = SigningKey::from_bytes(&[33; 32]);
        let key_id = "tamper-checksums";
        let module = b"tamper checksums module";
        let bytes = build_signed_handler_pack(&manifest(key_id, module), module, &signing).unwrap();
        let tampered = rebuild_with_tamper(&bytes, "checksums.json");
        let trust = TrustStore(BTreeMap::from([(key_id.into(), signing.verifying_key())]));
        // Tampering checksums bytes breaks JSON parsing, digest values, or
        // signature verification (signature covers canonical checksums). Any
        // of these rejections is correct.
        let result = load_handler_pack(std::io::Cursor::new(&tampered), &trust);
        assert!(result.is_err(), "tampered checksums must be rejected");
    }

    #[test]
    fn handler_pack_rejects_tampered_signature() {
        let signing = SigningKey::from_bytes(&[34; 32]);
        let key_id = "tamper-signature";
        let module = b"tamper signature module";
        let bytes = build_signed_handler_pack(&manifest(key_id, module), module, &signing).unwrap();
        let tampered = rebuild_with_tamper(&bytes, "META-INF/signature.ed25519");
        let trust = TrustStore(BTreeMap::from([(key_id.into(), signing.verifying_key())]));
        assert!(matches!(
            load_handler_pack(std::io::Cursor::new(&tampered), &trust),
            Err(HandlerPackError::Archive(ArchiveError::Signature))
        ));
    }

    #[test]
    fn handler_pack_rejects_unknown_publisher_key() {
        let signing = SigningKey::from_bytes(&[35; 32]);
        let key_id = "unknown-key-test";
        let module = b"unknown key module";
        let bytes = build_signed_handler_pack(&manifest(key_id, module), module, &signing).unwrap();
        // Empty trust store — key_id is not present at all.
        let trust = TrustStore(BTreeMap::new());
        assert!(matches!(
            load_handler_pack(std::io::Cursor::new(&bytes), &trust),
            Err(HandlerPackError::Archive(ArchiveError::UnknownKey))
        ));
    }

    #[test]
    fn handler_pack_skips_directory_entries() {
        let signing = SigningKey::from_bytes(&[36; 32]);
        let key_id = "dir-entry-test";
        let module = b"dir entry module";
        let bytes = build_signed_handler_pack(&manifest(key_id, module), module, &signing).unwrap();

        // Read entries from the valid pack.
        let entries = read_zip_entries(&bytes);
        let files: Vec<(&str, &[u8])> = entries
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_slice()))
            .collect();

        // Rebuild with a directory entry prepended. The `is_dir()` skip logic
        // in read_archive must allow the pack to load despite the extra
        // `META-INF/` directory entry (which is not in the allowed list).
        let mut output = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut output);
            let options = SimpleFileOptions::default();
            zip.add_directory("META-INF/", options).unwrap();
            for (name, data) in &files {
                zip.start_file(name, options).unwrap();
                zip.write_all(data).unwrap();
            }
            zip.finish().unwrap();
        }
        let rebuilt = output.into_inner();

        let trust = TrustStore(BTreeMap::from([(key_id.into(), signing.verifying_key())]));
        let result = load_handler_pack(std::io::Cursor::new(&rebuilt), &trust);
        assert!(
            result.is_ok(),
            "directory entry should be skipped: {:?}",
            result.err()
        );
    }

    #[test]
    fn handler_pack_rejects_oversized_file_in_zip() {
        // Build a ZIP with a handler.wasm file exceeding the 4 MiB
        // max_file_bytes limit. read_archive must reject it before manifest
        // validation runs.
        let oversized: Vec<u8> = vec![0u8; (4 * 1024 * 1024) + 1];
        let files: &[(&str, &[u8])] = &[
            ("manifest.json", b"{}"),
            ("handler.wasm", &oversized),
            ("checksums.json", b"{}"),
            ("META-INF/signature.ed25519", &[0u8; 64]),
        ];
        let bytes = build_malicious_zip(files);
        let trust = TrustStore(BTreeMap::new());
        assert!(matches!(
            load_handler_pack(std::io::Cursor::new(&bytes), &trust),
            Err(HandlerPackError::Archive(ArchiveError::Limit("file size")))
        ));
    }

    #[test]
    fn handler_pack_rejects_compression_bomb() {
        // Build a ZIP with a highly compressible handler.wasm (1 MiB of zeros,
        // Deflated → ~1 KiB). The compression ratio ~1000:1 exceeds the
        // max_compression_ratio limit of 100.
        let uncompressed_size: usize = 1024 * 1024;
        let bomb_payload: Vec<u8> = vec![0u8; uncompressed_size];
        let mut output = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut output);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("handler.wasm", options).unwrap();
            zip.write_all(&bomb_payload).unwrap();
            zip.finish().unwrap();
        }
        let bytes = output.into_inner();
        let trust = TrustStore(BTreeMap::new());
        assert!(matches!(
            load_handler_pack(std::io::Cursor::new(&bytes), &trust),
            Err(HandlerPackError::Archive(ArchiveError::Limit(
                "compression ratio"
            )))
        ));
    }

    #[test]
    fn handler_pack_rejects_load_time_module_digest_mismatch() {
        // Build a pack where manifest.moduleSha256 points to real_module but
        // the archive contains fake_module. checksums.json will correctly
        // cover fake_module (so checksum + signature verification passes), but
        // the load-time manifest-vs-module digest check triggers
        // ModuleDigestMismatch.
        let signing = SigningKey::from_bytes(&[37; 32]);
        let key_id = "load-digest-test";
        let real_module = b"real module for load digest test";
        let fake_module = b"fake module for load digest test";

        let manifest_json = serde_json::json!({
            "formatVersion": HANDLER_PACKAGE_FORMAT_VERSION,
            "id": "org.example.handler",
            "version": "1.0.0",
            "handlerApiVersion": HANDLER_API_VERSION,
            "minRuntimeVersion": "0.7.0",
            "publisherKeyId": key_id,
            "capabilities": ["org.example.predict"],
            "moduleSha256": hex::encode(Sha256::digest(real_module)),
            "moduleSize": real_module.len() as u64,
        });
        let manifest_bytes = serde_json::to_vec_pretty(&manifest_json).unwrap();
        let archive =
            build_signed_archive(&manifest_bytes, MODULE_PATH, fake_module, &signing).unwrap();

        let trust = TrustStore(BTreeMap::from([(key_id.into(), signing.verifying_key())]));
        assert!(matches!(
            load_handler_pack(std::io::Cursor::new(&archive), &trust),
            Err(HandlerPackError::ModuleDigestMismatch)
        ));
    }

    // ----- R-021: compatibility tests -----

    #[test]
    fn handler_pack_rejects_runtime_too_old() {
        // A handler requiring a newer runtime than the current version must be
        // rejected at load time with HandlerPackError::RuntimeTooOld.
        let signing = SigningKey::from_bytes(&[41; 32]);
        let key_id = "runtime-too-old-test";
        let module = b"module for runtime too old test";
        let mut bad_manifest = manifest(key_id, module);
        // Require a runtime version far in the future.
        bad_manifest.min_runtime_version = "999.0.0".into();
        let bytes = build_signed_handler_pack(&bad_manifest, module, &signing).unwrap();
        let trust = TrustStore(BTreeMap::from([(key_id.into(), signing.verifying_key())]));
        assert!(matches!(
            load_handler_pack(std::io::Cursor::new(&bytes), &trust),
            Err(HandlerPackError::RuntimeTooOld { .. })
        ));
    }
}
