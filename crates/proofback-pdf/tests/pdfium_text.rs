//! Requires PDFium (`cargo xtask pdfium`). Skipped when it is missing, except in CI
//! where `PROOFBACK_REQUIRE_PDFIUM=1` turns a missing library into a failure.

use std::path::Path;

use proofback_pdf::{TextExtractor, load_pdfium};

#[test]
fn extracts_positioned_text_from_a_pdflatex_build() {
    let pdfium = match load_pdfium(None) {
        Ok(p) => p,
        Err(e) if std::env::var_os("PROOFBACK_REQUIRE_PDFIUM").is_none() => {
            eprintln!("skipping: {e}");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/thesis-basic/main.pdf"),
    )
    .unwrap();
    let ex = TextExtractor::open(&pdfium, &bytes).unwrap();
    assert!(ex.page_count() >= 1);
    let page = ex.page(1).unwrap();
    let text = page.text();
    assert!(text.contains("estimator is strong"), "{text}");
    let c = page.chars.iter().find(|c| c.c == 'T').unwrap();
    assert!(c.bbox[2] > c.bbox[0] && c.bbox[3] > c.bbox[1]);
}
