#![no_main]

use ed25519_dalek::SigningKey;
use libfuzzer_sys::fuzz_target;
use rill_runtime::{TrustStore, sign_release_index, verify_release_index};
use rill_runtime_protocol::{
    RUNTIME_API_VERSION, ReleaseArtifact, ReleaseArtifactKind, ReleaseIndexPayload,
};
use std::collections::BTreeMap;

fn valid_payload(data: &[u8]) -> ReleaseIndexPayload {
    let mut payload = ReleaseIndexPayload {
        schema_version: 3,
        channel: "stable".into(),
        generated_at: "2026-08-22T00:00:00Z".into(),
        publisher_key_id: "fuzz-publisher".into(),
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
    };
    if let Some(byte) = data.first() {
        match byte % 5 {
            0 => payload.channel = "candidate".into(),
            1 => payload.generated_at = String::from_utf8_lossy(&data[..data.len().min(32)]).into_owned(),
            2 => payload.artifacts[0].url.push_str("?fuzz=1"),
            3 => payload.artifacts[0].size = u64::from(*byte),
            _ => payload.artifacts[0].sha256.replace_range(..2, "zz"),
        }
    }
    payload
}

fuzz_target!(|data: &[u8]| {
    // Keep the fully random parser path: arbitrary valid JSON may exercise
    // unknown-key, duplicate-identity, and shape failures before crypto.
    if let Ok(index) = serde_json::from_slice::<rill_runtime_protocol::SignedReleaseIndex>(data) {
        let _ = verify_release_index(&index, &TrustStore::default());
    }

    // The deterministic path installs the matching publisher key so fuzzed
    // payload fields reach canonicalisation and Ed25519 verification rather
    // than stopping at UnknownKey.
    let signing = SigningKey::from_bytes(&[42; 32]);
    let mut index = sign_release_index(valid_payload(&[]), &signing).unwrap();
    if let Some(byte) = data.first() {
        match byte % 5 {
            0 => index.payload.channel = "candidate".into(),
            1 => index.payload.generated_at = String::from_utf8_lossy(&data[..data.len().min(32)]).into_owned(),
            2 => index.payload.artifacts[0].url.push_str("?fuzz=1"),
            3 => index.payload.artifacts[0].size = u64::from(*byte),
            _ => index.payload.artifacts[0].sha256.replace_range(..2, "zz"),
        }
    }
    if data.first().is_some_and(|byte| byte & 1 == 1) {
        let mut signature = [0u8; 64];
        for (slot, byte) in signature.iter_mut().zip(data.iter().copied().cycle()) {
            *slot = byte;
        }
        index.signature = hex::encode(signature);
    }
    let mut trust = BTreeMap::new();
    trust.insert("fuzz-publisher".into(), signing.verifying_key());
    let _ = verify_release_index(&index, &TrustStore(trust));
});
