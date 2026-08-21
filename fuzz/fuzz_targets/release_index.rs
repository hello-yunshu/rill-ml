#![no_main]

use libfuzzer_sys::fuzz_target;
use rill_runtime::{TrustStore, verify_release_index};
use rill_runtime_protocol::SignedReleaseIndex;

fuzz_target!(|data: &[u8]| {
    if let Ok(index) = serde_json::from_slice::<SignedReleaseIndex>(data) {
        let _ = verify_release_index(&index, &TrustStore::default());
    }
});
