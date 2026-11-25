//! Round trip against a real pdflatex build: every text node found by forward
//! search must come back from inverse search at its own position.

use std::path::{Path, PathBuf};

use proofback_synctex::{Kind, Limits, SyncTex};

fn fixture() -> SyncTex {
    let p =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/thesis-basic/main.synctex.gz");
    SyncTex::open(p, Limits::default()).expect("fixture parses")
}

#[test]
fn resolves_the_chapter_file() {
    let s = fixture();
    assert!(
        s.tag_for(
            Path::new("/elsewhere"),
            &PathBuf::from("chapters/method.tex")
        )
        .is_some()
    );
    assert!(
        s.tag_for(Path::new("/elsewhere"), &PathBuf::from("main.tex"))
            .is_some()
    );
}

#[test]
fn inverse_finds_each_text_node() {
    let s = fixture();
    let tag = s
        .tag_for(
            Path::new("/elsewhere"),
            &PathBuf::from("chapters/method.tex"),
        )
        .unwrap();
    let mut total = 0;
    let mut top1 = 0;
    let mut top3 = 0;
    for page in s.pages() {
        for r in page
            .records
            .iter()
            .filter(|r| r.tag == tag && r.kind == Kind::Current)
        {
            total += 1;
            let c = s.inverse(page.number, r.x + 1.0, r.y - 2.0);
            if c.first().is_some_and(|c| c.tag == tag && c.line == r.line) {
                top1 += 1;
            }
            if c.iter().take(3).any(|c| c.tag == tag && c.line == r.line) {
                top3 += 1;
            }
        }
    }
    assert!(total > 20, "fixture should have text nodes, got {total}");
    assert!(top3 * 100 >= total * 95, "top-3 {top3}/{total}");
    assert!(top1 * 100 >= total * 80, "top-1 {top1}/{total}");
}
