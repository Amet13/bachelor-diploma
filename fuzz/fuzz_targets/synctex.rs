#![no_main]
use libfuzzer_sys::fuzz_target;
use proofback_synctex::{Limits, SyncTex};

fuzz_target!(|data: &[u8]| {
    let limits = Limits { max_bytes: 1 << 22, max_records: 100_000, ..Limits::default() };
    if let Ok(s) = SyncTex::parse(data, limits) {
        for p in s.pages() {
            let _ = s.inverse(p.number, 100.0, 100.0);
        }
    }
});
