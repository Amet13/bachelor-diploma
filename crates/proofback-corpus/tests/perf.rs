//! Performance budgets from the plan (section 11), on a ~300-page thesis with 500
//! annotations. Runs when `PROOFBACK_PERF_FIXTURE` points at a built fixture
//! (`python3 fixtures/generate.py --perf DIR` plus two pdflatex runs); CI does this.
//! Budgets are checked in release builds only.

use std::path::PathBuf;
use std::time::Instant;

use proofback_anchor::{Input, Sources, import};
use proofback_corpus::{Fixture, generate};
use proofback_synctex::SyncTex;

#[test]
fn import_meets_budgets() {
    let Some(dir) = std::env::var_os("PROOFBACK_PERF_FIXTURE").map(PathBuf::from) else {
        eprintln!("skipping: PROOFBACK_PERF_FIXTURE not set");
        return;
    };
    let pdfium = proofback_pdf::load_pdfium(None).expect("PDFium");
    let fx = Fixture::load(&dir).unwrap();
    assert!(
        fx.synctex.pages().len() >= 290,
        "fixture has {} pages",
        fx.synctex.pages().len()
    );
    let (pdf, truths) = generate(&fx, &pdfium, 500, 11).unwrap();
    assert!(truths.len() >= 480, "generated {}", truths.len());
    let synctex_bytes = std::fs::read(dir.join("main.synctex.gz")).unwrap();
    let mut files = Vec::new();
    for (f, src) in fx.sources.files() {
        files.push((f.to_owned(), src.text.clone()));
    }

    // Cold: parse the PDF, SyncTeX and sources from scratch, then anchor.
    let t0 = Instant::now();
    let extracted =
        proofback_pdf::extract(&pdfium, &pdf, proofback_pdf::Limits::default()).unwrap();
    let t_extract = t0.elapsed();
    let synctex = SyncTex::parse(
        synctex_bytes.as_slice(),
        proofback_synctex::Limits::default(),
    )
    .unwrap();
    let t_synctex = t0.elapsed().saturating_sub(t_extract);
    let sources = Sources::new(files.clone());
    let texts = extracted
        .texts
        .iter()
        .map(|t| (t.page, t.clone()))
        .collect();
    let input = Input {
        round: "perf",
        annotations: &extracted.annotations,
        boxes: &extracted.boxes,
        texts: &texts,
        synctex: Some(&synctex),
        project_root: &dir,
        sources: &sources,
        anonymize: false,
    };
    let items = import(&input);
    let cold = t0.elapsed();
    assert_eq!(items.len(), truths.len());

    // Warm: SyncTeX and sources already parsed (cached).
    let t1 = Instant::now();
    let items2 = import(&input);
    let warm = t1.elapsed();
    assert_eq!(items2.len(), items.len());

    // Sync against the unchanged sources.
    let mut items3 = items.clone();
    let t2 = Instant::now();
    let rep = proofback_store::sync::sync_items(&mut items3, &sources);
    let sync = t2.elapsed();
    assert_eq!(rep.lost, 0);

    let peak_mb = peak_rss_mb();
    eprintln!(
        "pages {} annotations {}: extract {:?}, synctex {:?}, cold import {:?}, warm import {:?}, sync {:?}, peak RSS {:?} MB",
        fx.synctex.pages().len(),
        items.len(),
        t_extract,
        t_synctex,
        cold,
        warm,
        sync,
        peak_mb
    );
    if cfg!(debug_assertions) {
        eprintln!("debug build: budgets not enforced");
        return;
    }
    assert!(cold.as_secs_f64() < 1.0, "cold import {cold:?} exceeds 1 s");
    assert!(
        warm.as_secs_f64() < 0.3,
        "warm import {warm:?} exceeds 300 ms"
    );
    assert!(sync.as_secs_f64() < 0.3, "sync {sync:?} exceeds 300 ms");
    if let Some(mb) = peak_mb {
        assert!(mb < 300, "peak memory {mb} MB exceeds 300 MB");
    }
}

fn peak_rss_mb() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024)
}
