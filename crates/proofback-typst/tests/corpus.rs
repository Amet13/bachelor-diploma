//! Typst accuracy (plan section 16, M8): the LaTeX gates apply. Needs PDFium.

use std::collections::HashMap;
use std::path::Path;

use proofback_anchor::Sources;
use proofback_model::{Confidence, Origin};
use proofback_pdf::{AnnotationSpec, Limits, TextExtractor, extract, write_annotations};

fn files() -> HashMap<String, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/typst-paper");
    let mut files = HashMap::new();
    files.insert(
        "main.typ".to_owned(),
        std::fs::read_to_string(root.join("main.typ")).unwrap(),
    );
    for e in std::fs::read_dir(root.join("sections")).unwrap() {
        let p = e.unwrap().path();
        files.insert(
            format!("sections/{}", p.file_name().unwrap().to_string_lossy()),
            std::fs::read_to_string(&p).unwrap(),
        );
    }
    files
}

#[test]
fn typst_accuracy_gates() {
    let pdfium = match proofback_pdf::load_pdfium(None) {
        Ok(p) => p,
        Err(e) if std::env::var_os("PROOFBACK_REQUIRE_PDFIUM").is_none() => {
            return eprintln!("skipping: {e}");
        }
        Err(e) => panic!("{e}"),
    };
    let files = files();
    let compiled = proofback_typst::compile(&files, "main.typ", None).unwrap();
    let pdf = proofback_typst::pdf(&compiled).unwrap();
    let ex = TextExtractor::open(&pdfium, &pdf).unwrap();
    let pages: Vec<_> = (1..=ex.page_count()).map(|p| ex.page(p).unwrap()).collect();
    let all_text: String = pages
        .iter()
        .map(proofback_pdf::PageText::text)
        .collect::<Vec<_>>()
        .join(" | ");
    let mut specs = Vec::new();
    let mut truth = Vec::new();
    let mut seed: u64 = 99;
    let mut rnd = |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    let mut attempts = 0;
    while truth.len() < 120 && attempts < 40_000 {
        attempts += 1;
        let pi = rnd(pages.len());
        let chars = &pages[pi].chars;
        if chars.len() < 50 {
            continue;
        }
        let start = rnd(chars.len() - 1);
        if !chars[start].c.is_alphabetic() || (start > 0 && chars[start - 1].c.is_alphanumeric()) {
            continue;
        }
        let words = 1 + rnd(3);
        let (mut end, mut seen) = (start, 0);
        while end < chars.len() {
            let c = chars[end].c;
            if !c.is_alphanumeric() {
                if c == ' ' {
                    seen += 1;
                    if seen == words {
                        break;
                    }
                } else {
                    break;
                }
            }
            end += 1;
        }
        let phrase: String = chars[start..end]
            .iter()
            .map(|c| c.c)
            .collect::<String>()
            .trim_end()
            .to_owned();
        let end = start + phrase.chars().count();
        if phrase.len() < 5 || all_text.matches(&phrase).count() != 1 {
            continue;
        }
        let hits: Vec<(&String, usize)> = files
            .iter()
            .flat_map(|(f, t)| t.match_indices(&phrase).map(move |(i, _)| (f, i)))
            .collect();
        if hits.len() != 1
            || truth.iter().any(
                |(_, _, p, _): &(String, std::ops::Range<usize>, usize, u8)| {
                    *p == pi * 100_000 + start / 50
                },
            )
        {
            continue;
        }
        let (file, at) = hits[0];
        let mut bbox = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for c in &chars[start..end] {
            bbox = [
                bbox[0].min(c.bbox[0]),
                bbox[1].min(c.bbox[1]),
                bbox[2].max(c.bbox[2]),
                bbox[3].max(c.bbox[3]),
            ];
        }
        let kind = (truth.len() % 4) as u8;
        let mut spec = AnnotationSpec::new(
            pi as u32 + 1,
            ["Highlight", "StrikeOut", "Underline", "Caret"][kind as usize],
            bbox,
        );
        spec.nm = Some(format!("t{}", truth.len()));
        if kind < 3 {
            spec.quads = vec![[
                bbox[0], bbox[3], bbox[2], bbox[3], bbox[0], bbox[1], bbox[2], bbox[1],
            ]];
        } else {
            let last = chars[end - 1];
            spec.rect = [
                last.bbox[2] - 2.0,
                last.bbox[1] - 1.0,
                last.bbox[2] + 2.0,
                last.bbox[1] + 4.0,
            ];
            spec.contents = Some("and more".into());
        }
        let range = if kind == 3 {
            at + phrase.len()..at + phrase.len()
        } else {
            at..at + phrase.len()
        };
        specs.push(spec);
        truth.push((file.clone(), range, pi * 100_000 + start / 50, kind));
    }
    assert!(truth.len() >= 80, "only {} spans", truth.len());
    let annotated = write_annotations(&pdf, &specs).unwrap();
    let x = extract(&pdfium, &annotated, Limits::default()).unwrap();
    let texts = x.texts.into_iter().map(|t| (t.page, t)).collect();
    let sources = Sources::new(files.clone());
    let items = proofback_typst::import(
        "typst",
        &x.annotations,
        &x.boxes,
        &texts,
        &compiled,
        &sources,
        false,
    );
    let (mut exact, mut wrong) = (0, 0);
    for (i, (file, range, _, _)) in truth.iter().enumerate() {
        let nm = format!("t{i}");
        let it = items
            .iter()
            .find(|it| matches!(&it.origin, Origin::Pdf { nm: Some(n), .. } if n == &nm))
            .unwrap();
        let ok = it
            .target
            .as_ref()
            .is_some_and(|t| &t.file == file && t.range.as_range() == *range);
        match it.anchor.confidence {
            Confidence::Exact if ok => exact += 1,
            Confidence::Exact => {
                wrong += 1;
                eprintln!(
                    "wrong: {nm} expected {file}:{range:?} got {:?}",
                    it.target.as_ref().map(|t| (&t.file, &t.range))
                );
            }
            c => eprintln!(
                "not exact: {nm} kind {} {c:?} {}",
                truth[i].3, it.anchor.reason
            ),
        }
    }
    let n = truth.len();
    eprintln!("Typst corpus n={n}: exact {exact}, wrong {wrong}");
    assert!(wrong * 100 < n, "wrong-anchor rate {wrong}/{n}");
    assert!(exact * 10 >= n * 9, "exact {exact}/{n}");
}
