//! Word round trip: `docx export` and importing a reviewed DOCX.

use std::path::Path;
use std::process::Command;

use proofback_model::{Item, Round, RoundFile, Source, SourceKind};
use proofback_store::views::row;
use proofback_store::{Error, Project, Result, merge_reimport, sha256_hex, slug, write_atomic};

use crate::commands::{now, read_limited};
use crate::output::{Style, table};
use crate::{DocxCmd, DocxEngine, ImportArgs, git};

pub fn run(p: &Project, cmd: &DocxCmd, quiet: bool) -> Result<u8> {
    let DocxCmd::Export {
        output,
        engine,
        title,
    } = cmd;
    let (sources, _) = p.load_sources()?;
    let paras = proofback_docx::paragraphs(&sources, &p.config.main);
    if paras.is_empty() {
        return Err(Error::Input(format!(
            "no paragraphs found from {}",
            p.config.main
        )));
    }
    let (bytes, anchored) = match engine {
        DocxEngine::Builtin => (
            proofback_docx::export(&paras, title).map_err(|e| Error::Input(e.to_string()))?,
            paras.len(),
        ),
        DocxEngine::Pandoc => {
            let tmp = p.cache_dir().join("pandoc-export.docx");
            std::fs::create_dir_all(p.cache_dir()).map_err(|e| Error::Input(e.to_string()))?;
            let status = Command::new("pandoc")
                .arg(&p.config.main)
                .arg("--output")
                .arg(&tmp)
                .current_dir(&p.root)
                .status()
                .map_err(|e| {
                    Error::Input(format!(
                        "cannot run pandoc ({e}); install it or use --engine builtin"
                    ))
                })?;
            if !status.success() {
                return Err(Error::Input("pandoc failed; try --engine builtin".into()));
            }
            let raw = std::fs::read(&tmp).map_err(|e| Error::Input(e.to_string()))?;
            let _ = std::fs::remove_file(&tmp);
            proofback_docx::add_bookmarks(&raw, &paras).map_err(|e| Error::Input(e.to_string()))?
        }
    };
    write_atomic(output, &bytes)?;
    if !quiet {
        println!(
            "Wrote {} ({} of {} paragraphs carry source anchors). Send it to your reviewer, then: proofback import <reviewed.docx>",
            output.display(),
            anchored,
            paras.len()
        );
    }
    Ok(0)
}

pub fn import(p: &Project, a: &ImportArgs, quiet: bool, style: &Style) -> Result<u8> {
    let bytes = read_limited(&a.file, p.config.limits.max_pdf_mb)?;
    let (sources, skipped) = p.load_sources()?;
    for s in &skipped {
        eprintln!("warning: skipped {s}");
    }
    let stem = a
        .file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let round_name = a
        .round
        .clone()
        .unwrap_or_else(|| format!("{}-{}", &now()[..10], slug(&stem)));
    p.round_path(&round_name)?;
    let items = proofback_docx::import(&bytes, &sources, &round_name)
        .map_err(|e| Error::Input(format!("{}: {e}", a.file.display())))?;
    let existing = p.rounds()?.into_iter().find(|r| r.round.name == round_name);
    let items = merge_reimport(existing.as_ref(), items);
    let round = Round {
        name: round_name.clone(),
        created: existing
            .as_ref()
            .map_or_else(now, |r| r.round.created.clone()),
        source: Source {
            kind: SourceKind::Docx,
            file: Path::new(&a.file).to_string_lossy().into_owned(),
            sha256: sha256_hex(&bytes),
            synctex: None,
            synctex_sha256: None,
            commit: git::head(&p.root),
        },
        anonymized: false,
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
            "Imported {} tracked change(s) and comment(s) into round {}.",
            rf.items.len(),
            style.bold(&round_name)
        );
        let refs: Vec<&Item> = rf.items.iter().collect();
        print!("{}", table(&refs, style));
    }
    Ok(0)
}
