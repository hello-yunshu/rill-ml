#![no_main]

use libfuzzer_sys::fuzz_target;
use rill_runtime_protocol::{RuntimeRequest, RuntimeResponse, RuntimeResponseV2};

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<RuntimeRequest>(data);
    let _ = serde_json::from_slice::<RuntimeResponse>(data);
    let _ = serde_json::from_slice::<RuntimeResponseV2>(data);
});
