//! Accuracy gates from the plan (§12), on the synthetic corpus:
//! - wrong-anchor rate under 1% (with and without SyncTeX);
//! - exact-range recall of at least 90% with SyncTeX.
//!
//! Needs PDFium; skipped when it is missing unless `PROOFBACK_REQUIRE_PDFIUM=1`.

use std::path::Path;

use proofback_corpus::{Fixture, Metrics, evaluate, generate};

fn pdfium() -> Option<pdfium_render::prelude::Pdfium> {
    match proofback_pdf::load_pdfium(None) {
        Ok(p) => Some(p),
        Err(e) if std::env::var_os("PROOFBACK_REQUIRE_PDFIUM").is_none() => {
            eprintln!("skipping: {e}");
            None
        }
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn accuracy_gates() {
    let Some(pdfium) = pdfium() else { return };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let mut with = Metrics::default();
    let mut without = Metrics::default();
    for name in ["thesis-long", "article-twocolumn", "thesis-basic"] {
        let fx = Fixture::load(&root.join(name)).unwrap();
        let count = if name == "thesis-basic" { 16 } else { 120 };
        let (pdf, truths) = generate(&fx, &pdfium, count, 42).unwrap();
        assert!(
            truths.len() >= count / 2,
            "{name}: only {} annotations generated",
            truths.len()
        );
        let (m, items) = evaluate(&fx, &pdfium, &pdf, &truths, true).unwrap();
        if std::env::var_os("PROOFBACK_CORPUS_DEBUG").is_some() {
            for t in &truths {
                let it = items.iter().find(|i| matches!(&i.origin, proofback_model::Origin::Pdf { nm: Some(n), .. } if n == &t.nm));
                eprintln!(
                    "{name} {:?} truth {}:{:?} -> {:?}",
                    t.case,
                    t.file,
                    t.range,
                    it.map(|i| (
                        &i.anchor.confidence,
                        i.target.as_ref().map(|t| (&t.file, &t.range)),
                        &i.anchor.reason
                    ))
                );
            }
        }
        eprintln!("{name} with SyncTeX: {m:?}");
        with.merge(&m);
        let (m2, _) = evaluate(&fx, &pdfium, &pdf, &truths, false).unwrap();
        eprintln!("{name} text only: {m2:?}");
        without.merge(&m2);
    }
    eprintln!(
        "WITH SYNCTEX: exact recall {:.3}, line {:.3}, wrong {:.3}, page {:.3} (n={})",
        with.exact_recall(),
        with.line_rate(),
        with.wrong_rate(),
        with.page_rate(),
        with.total
    );
    eprintln!(
        "TEXT ONLY:    exact recall {:.3}, line {:.3}, wrong {:.3}, page {:.3} (n={})",
        without.exact_recall(),
        without.line_rate(),
        without.wrong_rate(),
        without.page_rate(),
        without.total
    );
    assert!(
        with.wrong_rate() < 0.01,
        "wrong-anchor rate with SyncTeX {:.3}",
        with.wrong_rate()
    );
    assert!(
        without.wrong_rate() < 0.01,
        "wrong-anchor rate text-only {:.3}",
        without.wrong_rate()
    );
    assert!(
        with.exact_recall() >= 0.90,
        "exact recall with SyncTeX {:.3}",
        with.exact_recall()
    );
}
