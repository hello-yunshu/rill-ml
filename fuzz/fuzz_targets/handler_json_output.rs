#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Handler output is untrusted JSON. The runtime's bounded parser must be
    // able to inspect arbitrary bytes without panicking before WIT validation.
    let _ = serde_json::from_slice::<serde_json::Value>(data);
});
