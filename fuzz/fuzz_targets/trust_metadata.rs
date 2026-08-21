#![no_main]

use libfuzzer_sys::fuzz_target;
use rill_runtime_protocol::TrustMetadataV1;

fuzz_target!(|data: &[u8]| {
    if let Ok(metadata) = serde_json::from_slice::<TrustMetadataV1>(data) {
        let _ = metadata.validate_shape();
        let _ = metadata.active_keys_at(1_735_689_600_000);
    }
});
