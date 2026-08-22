#![no_main]

use ed25519_dalek::SigningKey;
use libfuzzer_sys::fuzz_target;
use rill_runtime::{
    sign_release_index_with_generation, trust_metadata_digest,
    verify_release_index_with_trust_metadata,
};
use rill_runtime_protocol::{
    RUNTIME_API_VERSION, ReleaseArtifact, ReleaseArtifactKind, ReleaseIndexPayload,
    TRUST_METADATA_SCHEMA_VERSION, TrustKeyMetadataV1, TrustKeyRole, TrustMetadataV1,
    TrustVerificationFloorV1,
};

fn metadata(current: &SigningKey, next: &SigningKey) -> TrustMetadataV1 {
    TrustMetadataV1 {
        schema_version: TRUST_METADATA_SCHEMA_VERSION,
        metadata_generation: 1,
        minimum_release_generation: 1,
        keys: vec![
            TrustKeyMetadataV1 {
                key_id: "fuzz-current".into(),
                public_key_hex: hex::encode(current.verifying_key().to_bytes()),
                role: TrustKeyRole::Current,
                not_before_unix_ms: 0,
                not_after_unix_ms: None,
                revoked_at_unix_ms: None,
                emergency_revoked: false,
            },
            TrustKeyMetadataV1 {
                key_id: "fuzz-next".into(),
                public_key_hex: hex::encode(next.verifying_key().to_bytes()),
                role: TrustKeyRole::Next,
                not_before_unix_ms: 0,
                not_after_unix_ms: None,
                revoked_at_unix_ms: None,
                emergency_revoked: false,
            },
        ],
    }
}

fn payload(key_id: &str) -> ReleaseIndexPayload {
    ReleaseIndexPayload {
        schema_version: 3,
        channel: "stable".into(),
        generated_at: "2026-08-22T00:00:00Z".into(),
        publisher_key_id: key_id.into(),
        artifacts: vec![ReleaseArtifact {
            kind: ReleaseArtifactKind::Runtime,
            id: "rill-runtime".into(),
            version: "1.3.0".into(),
            runtime_api_version: RUNTIME_API_VERSION,
            target_os: Some("linux".into()),
            target_arch: Some("x86_64".into()),
            target_libc: Some("gnu".into()),
            handler_api_version: None,
            min_runtime_version: None,
            pm_adapter_protocol_version: None,
            url: "https://example.invalid/rill-runtime".into(),
            sha256: "ab".repeat(32),
            size: 1024,
        }],
    }
}

fuzz_target!(|data: &[u8]| {
    if let Ok(candidate) = serde_json::from_slice::<TrustMetadataV1>(data) {
        let _ = candidate.validate_shape();
        let _ = candidate.active_keys_at(1_735_689_600_000);
    }

    let current = SigningKey::from_bytes(&[43; 32]);
    let next = SigningKey::from_bytes(&[44; 32]);
    let mut trust = metadata(&current, &next);
    let first = data.first().copied().unwrap_or_default();
    trust.metadata_generation = 1 + u64::from(first % 4);
    trust.minimum_release_generation = 1 + u64::from(first % 3);
    trust.keys[1].not_before_unix_ms = u64::from(first);
    trust.keys[1].revoked_at_unix_ms = if first & 2 == 2 { Some(100) } else { None };
    trust.keys[1].emergency_revoked = first & 4 == 4;
    if first & 8 == 8 {
        trust.keys[0].role = TrustKeyRole::Next;
        trust.keys[1].role = TrustKeyRole::Current;
    }
    let signing = if matches!(&trust.keys[0].role, &TrustKeyRole::Current) {
        &current
    } else {
        &next
    };
    let publisher = if std::ptr::eq(signing, &current) { "fuzz-current" } else { "fuzz-next" };
    let envelope = sign_release_index_with_generation(
        payload(publisher),
        1 + u64::from(data.get(1).copied().unwrap_or_default() % 4),
        signing,
    );
    let Ok(envelope) = envelope else { return };
    let floor = TrustVerificationFloorV1 {
        minimum_metadata_generation: 1,
        minimum_release_generation: 1,
        metadata_digest: Some(trust_metadata_digest(&metadata(&current, &next)).unwrap()),
    };
    let _ = verify_release_index_with_trust_metadata(&envelope, &trust, &floor, 100);
});
