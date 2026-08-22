#![no_main]

use libfuzzer_sys::fuzz_target;
use rill_ml::{Snapshot, stats::Mean};

fuzz_target!(|data: &[u8]| {
    if let Ok(json) = std::str::from_utf8(data) {
        let _ = Snapshot::<Mean>::from_json_validated(json);
    }
});
