#![no_main]

use libfuzzer_sys::fuzz_target;
use rill_runtime_protocol::v3::EnvelopeV3;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<EnvelopeV3>(data);
});
