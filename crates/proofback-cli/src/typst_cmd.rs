//! Typst projects: anchoring through the embedded compiler's glyph index.

use std::collections::HashMap;

use proofback_anchor::Sources;
use proofback_model::{Item, Round, RoundFile, Source, SourceKind};
use proofback_pdf::{PageBox, PageText, PdfAnnotation};
use proofback_store::views::row;
use proofback_store::{Error, Project, Result, merge_reimport, sha256_hex};

use crate::commands::now;
use crate::output::{Style, table};
use crate::{ImportArgs, git};

#[allow(clippy::too_many_arguments)]
pub fn import_pdf(
    p: &Project,
    a: &ImportArgs,
    quiet: bool,
    style: &Style,
    round_name: &str,
    annotations: &[PdfAnnotation],
    boxes: &[PageBox],
    texts: &HashMap<u32, PageText>,
    sources: &Sources,
    pdf_bytes: &[u8],
) -> Result<u8> {
    let items = anchor(
        p,
        round_name,
        annotations,
        boxes,
        texts,
        sources,
        a.anonymize_authors || p.config.import.anonymize_authors,
    )?;
    let existing = p.rounds()?.into_iter().find(|r| r.round.name == round_name);
    let items = merge_reimport(existing.as_ref(), items);
    let round = Round {
        name: round_name.to_owned(),
        created: existing
            .as_ref()
            .map_or_else(now, |r| r.round.created.clone()),
        source: Source {
            kind: SourceKind::Pdf,
            file: a.file.to_string_lossy().into_owned(),
            sha256: sha256_hex(pdf_bytes),
            synctex: None,
            synctex_sha256: None,
            commit: git::head(&p.root),
        },
        anonymized: a.anonymize_authors,
    };
    let rf = RoundFile::new(round, items);
    p.save_round(&rf)?;
    if a.json {
        let rows: Vec<_> = rf.items.iter().map(row).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({ "round": round_name, "items": rows })
            )
            .expect("json")
        );
    } else if !quiet {
        println!(
            "Imported {} item(s) into round {} (Typst glyph index).",
            rf.items.len(),
            style.bold(round_name)
        );
        let refs: Vec<&Item> = rf.items.iter().collect();
        print!("{}", table(&refs, style));
    }
    Ok(0)
}

#[cfg(feature = "typst")]
fn anchor(
    p: &Project,
    round: &str,
    annotations: &[PdfAnnotation],
    boxes: &[PageBox],
    texts: &HashMap<u32, PageText>,
    sources: &Sources,
    anonymize: bool,
) -> Result<Vec<Item>> {
    let files: HashMap<String, String> = sources
        .files()
        .filter(|(f, _)| {
            std::path::Path::new(f)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("typ"))
        })
        .map(|(f, s)| (f.to_owned(), s.text.clone()))
        .collect();
    let compiled =
        proofback_typst::compile(&files, &p.config.main, Some(&p.root)).map_err(Error::Input)?;
    Ok(proofback_typst::import(
        round,
        annotations,
        boxes,
        texts,
        &compiled,
        sources,
        anonymize,
    ))
}

#[cfg(not(feature = "typst"))]
fn anchor(
    _: &Project,
    _: &str,
    _: &[PdfAnnotation],
    _: &[PageBox],
    _: &HashMap<u32, PageText>,
    _: &Sources,
    _: bool,
) -> Result<Vec<Item>> {
    Err(Error::Usage(
        "this proofback was built without Typst support".into(),
    ))
}
