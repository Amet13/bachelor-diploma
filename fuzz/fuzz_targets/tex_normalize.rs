#![no_main]
use libfuzzer_sys::fuzz_target;
use proofback_tex::{collect_macros, normalize, Options};

fuzz_target!(|data: &str| {
    let opts = Options { macros: collect_macros(data) };
    let n = normalize(data, &opts);
    for i in 0..n.len() {
        let r = n.span(i);
        assert!(r.start <= r.end && r.end <= data.len());
        assert!(data.is_char_boundary(r.start) && data.is_char_boundary(r.end));
    }
});
