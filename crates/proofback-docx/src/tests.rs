use std::io::{Read, Write};
use std::path::Path;

use proofback_anchor::Sources;
use proofback_model::{Confidence, Kind};

use super::*;

fn fixture_sources() -> Sources {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/thesis-long");
    let mut files = vec![(
        "main.tex".to_owned(),
        std::fs::read_to_string(root.join("main.tex")).unwrap(),
    )];
    let mut chapters: Vec<_> = std::fs::read_dir(root.join("chapters"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    chapters.sort();
    for c in chapters {
        files.push((
            format!("chapters/{}", c.file_name().unwrap().to_string_lossy()),
            std::fs::read_to_string(&c).unwrap(),
        ));
    }
    Sources::new(files)
}

fn read_part(docx: &[u8], name: &str) -> String {
    let mut ar = zip::ZipArchive::new(std::io::Cursor::new(docx)).unwrap();
    let mut s = String::new();
    ar.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

fn replace_parts(docx: &[u8], parts: &[(&str, String)]) -> Vec<u8> {
    let mut ar = zip::ZipArchive::new(std::io::Cursor::new(docx)).unwrap();
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut out);
        let o = zip::write::SimpleFileOptions::default();
        let mut written = Vec::new();
        for i in 0..ar.len() {
            let mut f = ar.by_index(i).unwrap();
            let name = f.name().unwrap().into_owned();
            let mut data = Vec::new();
            f.read_to_end(&mut data).unwrap();
            if let Some((_, new)) = parts.iter().find(|(n, _)| *n == name) {
                data = new.clone().into_bytes();
                written.push(name.clone());
            }
            z.start_file(name, o).unwrap();
            z.write_all(&data).unwrap();
        }
        for (n, d) in parts {
            if !written.iter().any(|w| w == n) {
                z.start_file(*n, o).unwrap();
                z.write_all(d.as_bytes()).unwrap();
            }
        }
        z.finish().unwrap();
    }
    out.into_inner()
}

/// Phrases that occur exactly once in the sources and in the exported text.
fn unique_phrases(
    s: &Sources,
    paras: &[Paragraph],
    n: usize,
) -> Vec<(String, String, std::ops::Range<usize>)> {
    let mut out = Vec::new();
    for p in paras.iter().filter(|p| p.heading.is_none()) {
        let mut in_para = 0;
        let words: Vec<&str> = p.text.split_whitespace().collect();
        for w in words.windows(4).step_by(7) {
            let phrase = w.join(" ");
            if !phrase.chars().all(|c| c.is_alphabetic() || c == ' ') {
                continue;
            }
            let folded: String = proofback_tex::fold_str(phrase.chars())
                .chars
                .iter()
                .collect();
            let hits: Vec<_> = s
                .files()
                .flat_map(|(f, src)| {
                    src.find_folded(&folded)
                        .into_iter()
                        .map(move |r| (f.to_owned(), r))
                })
                .collect();
            let in_doc = paras
                .iter()
                .map(|q| q.text.matches(&phrase).count())
                .sum::<usize>();
            if hits.len() == 1 && in_doc == 1 {
                let (f, r) = &hits[0];
                let br = s
                    .get(f)
                    .unwrap()
                    .source_range_for_folded(r.clone())
                    .unwrap();
                out.push((phrase, f.clone(), br));
                in_para += 1;
                if in_para == 3 || out.len() >= n {
                    break;
                }
            }
        }
        if out.len() >= n {
            break;
        }
    }
    out
}

#[test]
fn export_has_anchors_and_round_trips_changes() {
    let s = fixture_sources();
    let paras = paragraphs(&s, "main.tex");
    assert!(paras.len() > 50, "{}", paras.len());
    assert!(
        paras
            .iter()
            .any(|p| p.heading == Some(1) && p.text == "Introduction")
    );
    assert!(
        !paras.iter().any(|p| p.text.contains("documentclass")),
        "preamble excluded"
    );
    let docx = export(&paras, "Robust Thresholds").unwrap();
    let xml = read_part(&docx, "word/document.xml");
    assert!(xml.contains("w:name=\"pb_"));

    let targets = unique_phrases(&s, &paras, 4);
    assert_eq!(targets.len(), 4);
    let mut xml2 = xml.clone();
    let wrap = |xml: &mut String, phrase: &str, with: String| {
        assert_eq!(xml.matches(phrase).count(), 1, "{phrase}");
        *xml = xml.replacen(
            phrase,
            &format!("</w:t></w:r>{with}<w:r><w:t xml:space=\"preserve\">"),
            1,
        );
    };
    let (p0, p1, p2, p3) = (&targets[0].0, &targets[1].0, &targets[2].0, &targets[3].0);
    wrap(
        &mut xml2,
        p0,
        format!(
            "<w:del w:id=\"11\" w:author=\"Prof. Hale\" w:date=\"2026-11-20T10:00:00Z\"><w:r><w:delText>{p0}</w:delText></w:r></w:del>"
        ),
    );
    wrap(
        &mut xml2,
        p1,
        format!(
            "<w:del w:id=\"12\" w:author=\"Prof. Hale\"><w:r><w:delText>{p1}</w:delText></w:r></w:del><w:ins w:id=\"13\" w:author=\"Prof. Hale\"><w:r><w:t>clearer words</w:t></w:r></w:ins>"
        ),
    );
    wrap(
        &mut xml2,
        p2,
        format!(
            "<w:r><w:t xml:space=\"preserve\">{p2}</w:t></w:r><w:ins w:id=\"14\" w:author=\"Dr. Okafor\"><w:r><w:t xml:space=\"preserve\"> and more</w:t></w:r></w:ins>"
        ),
    );
    wrap(
        &mut xml2,
        p3,
        format!(
            "<w:commentRangeStart w:id=\"5\"/><w:r><w:t xml:space=\"preserve\">{p3}</w:t></w:r><w:commentRangeEnd w:id=\"5\"/>"
        ),
    );
    let comments = "<?xml version=\"1.0\"?><w:comments xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:comment w:id=\"5\" w:author=\"Dr. Okafor\" w:date=\"2026-11-21T09:00:00Z\"><w:p><w:r><w:t>Is this really true &amp; tested?</w:t></w:r></w:p></w:comment></w:comments>".to_owned();
    let reviewed = replace_parts(
        &docx,
        &[
            ("word/document.xml", xml2.clone()),
            ("word/comments.xml", comments.clone()),
        ],
    );

    let items = import(&reviewed, &s, "w1").unwrap();
    assert_eq!(items.len(), 4, "{items:#?}");
    let find = |k: Kind| items.iter().find(|i| i.kind == k).unwrap();
    let del = find(Kind::Delete);
    assert_eq!(del.anchor.confidence, Confidence::Exact, "{del:?}");
    assert_eq!(del.target.as_ref().unwrap().range.as_range(), targets[0].2);
    assert_eq!(del.author.as_deref(), Some("Prof. Hale"));
    let rep = find(Kind::Replace);
    assert_eq!(rep.target.as_ref().unwrap().range.as_range(), targets[1].2);
    assert_eq!(
        rep.suggestion.as_ref().unwrap().replace_with.as_deref(),
        Some("clearer words")
    );
    let ins = find(Kind::Insert);
    let t = ins.target.as_ref().unwrap();
    assert_eq!(t.range.start, targets[2].2.end, "{ins:?}");
    assert_eq!(t.range.start, t.range.end);
    let q = find(Kind::Question);
    assert_eq!(q.target.as_ref().unwrap().range.as_range(), targets[3].2);
    assert_eq!(q.body, "Is this really true & tested?");

    // Without anchors (bookmarks removed), unique text still resolves.
    let no_marks: String = {
        let mut out = String::new();
        let mut rest = xml2.as_str();
        while let Some(i) = rest.find("<w:bookmark") {
            out.push_str(&rest[..i]);
            let end = rest[i..].find("/>").unwrap() + i + 2;
            rest = &rest[end..];
        }
        out.push_str(rest);
        out
    };
    let bare = replace_parts(
        &docx,
        &[
            ("word/document.xml", no_marks),
            ("word/comments.xml", comments),
        ],
    );
    let items = import(&bare, &s, "w2").unwrap();
    assert_eq!(
        items
            .iter()
            .filter(|i| i.anchor.confidence == Confidence::Exact)
            .count(),
        4
    );
}

#[test]
fn pandoc_export_gets_anchors_when_available() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/thesis-long");
    let tmp = std::env::temp_dir().join(format!("pb-pandoc-{}.docx", std::process::id()));
    let Ok(status) = std::process::Command::new("pandoc")
        .arg("main.tex")
        .arg("-o")
        .arg(&tmp)
        .current_dir(&root)
        .status()
    else {
        eprintln!("skipping: pandoc not installed");
        return;
    };
    assert!(status.success());
    let s = fixture_sources();
    let paras = paragraphs(&s, "main.tex");
    let (docx, added) = add_bookmarks(&std::fs::read(&tmp).unwrap(), &paras).unwrap();
    let _ = std::fs::remove_file(&tmp);
    assert!(
        added * 2 > paras.len(),
        "only {added} of {} paragraphs anchored",
        paras.len()
    );
    assert!(read_part(&docx, "word/document.xml").contains("w:name=\"pb_"));
    assert!(read_part(&docx, "customXml/item1.xml").contains("chapters/"));
}

#[test]
fn rejects_non_docx() {
    assert!(import(b"not a zip", &Sources::default(), "r").is_err());
}

/// M7 exit criterion: round trip on fixtures with at least 90% exact anchors and
/// no wrong ones.
#[test]
fn round_trip_corpus_meets_the_gate() {
    let s = fixture_sources();
    let paras = paragraphs(&s, "main.tex");
    let docx = export(&paras, "").unwrap();
    let mut xml = read_part(&docx, "word/document.xml");
    let targets = unique_phrases(&s, &paras, 60);
    assert!(targets.len() >= 40, "{}", targets.len());
    let mut comments = String::from(
        "<?xml version=\"1.0\"?><w:comments xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
    );
    for (i, (phrase, _, _)) in targets.iter().enumerate() {
        let with = match i % 4 {
            0 => format!(
                "<w:del w:id=\"{i}\" w:author=\"A\"><w:r><w:delText>{phrase}</w:delText></w:r></w:del>"
            ),
            1 => format!(
                "<w:del w:id=\"{i}\" w:author=\"A\"><w:r><w:delText>{phrase}</w:delText></w:r></w:del><w:ins w:id=\"{}\" w:author=\"A\"><w:r><w:t>new text</w:t></w:r></w:ins>",
                i + 1000
            ),
            2 => format!(
                "<w:r><w:t xml:space=\"preserve\">{phrase}</w:t></w:r><w:ins w:id=\"{i}\" w:author=\"A\"><w:r><w:t xml:space=\"preserve\"> added</w:t></w:r></w:ins>"
            ),
            _ => {
                comments.push_str(&format!("<w:comment w:id=\"{i}\" w:author=\"B\"><w:p><w:r><w:t>Note {i}</w:t></w:r></w:p></w:comment>"));
                format!(
                    "<w:commentRangeStart w:id=\"{i}\"/><w:r><w:t xml:space=\"preserve\">{phrase}</w:t></w:r><w:commentRangeEnd w:id=\"{i}\"/>"
                )
            }
        };
        xml = xml.replacen(
            phrase.as_str(),
            &format!("</w:t></w:r>{with}<w:r><w:t xml:space=\"preserve\">"),
            1,
        );
    }
    comments.push_str("</w:comments>");
    let reviewed = replace_parts(
        &docx,
        &[("word/document.xml", xml), ("word/comments.xml", comments)],
    );
    let items = import(&reviewed, &s, "corpus").unwrap();
    assert_eq!(items.len(), targets.len());
    let mut exact = 0;
    let mut wrong = 0;
    for it in &items {
        let Some(t) = &it.target else { continue };
        let Some((_, file, range)) = targets
            .iter()
            .find(|(_, f, r)| f == &t.file && (r.start..=r.end).contains(&t.range.start))
        else {
            wrong += 1;
            continue;
        };
        let ok = if it.kind == Kind::Insert {
            t.range.start == range.end
        } else {
            t.range.as_range() == *range && &t.file == file
        };
        match (it.anchor.confidence, ok) {
            (Confidence::Exact, true) => exact += 1,
            (Confidence::Exact, false) => wrong += 1,
            _ => {}
        }
    }
    eprintln!("docx corpus: {exact}/{} exact, {wrong} wrong", items.len());
    assert_eq!(wrong, 0);
    assert!(
        exact * 10 >= items.len() * 9,
        "exact {exact}/{}",
        items.len()
    );
}
