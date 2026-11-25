//! WebAssembly bindings for the browser app and the npm package.
//!
//! Inputs and outputs are JSON strings or byte arrays, so the JavaScript side stays
//! small. Everything runs locally; nothing here performs network access.
//!
//! PDF text extraction uses PDFium's WebAssembly build: call
//! `initialize_pdfium_render(pdfiumModule, wasmModule, false)` from JavaScript
//! (exported by `pdfium-render`) before [`import_review`] or [`render_page`].

use std::collections::{BTreeMap, HashMap};
use std::io::Read;

use pdfium_render::prelude::{PdfRenderConfig, Pdfium};
use proofback_anchor::{Input, Sources, import};
use proofback_emit::{LetterFormat, answer_pdf, letter, markdown_checklist, sarif};
use proofback_model::{Item, Round, RoundFile, Source, SourceKind};
use proofback_store::edit::{apply_in_memory, edit_for, unified_diff};
use proofback_synctex::SyncTex;
use wasm_bindgen::prelude::*;

/// Limits for untrusted archives.
const MAX_ZIP_ENTRIES: usize = 20_000;
const MAX_ENTRY_BYTES: u64 = 64 << 20;
const MAX_TOTAL_BYTES: u64 = 512 << 20;

fn err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Unpack a project zip (for example Overleaf's download). Returns JSON
/// `{ "sources": {path: text}, "synctex": [path], "pdf": [path], "skipped": [..] }`.
/// Paths are confined to the archive root; oversized or non-UTF-8 files are skipped.
#[wasm_bindgen]
#[allow(clippy::case_sensitive_file_extension_comparisons)] // compared after lower-casing
pub fn read_zip(bytes: &[u8]) -> Result<String, JsError> {
    let mut ar = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(err)?;
    if ar.len() > MAX_ZIP_ENTRIES {
        return Err(err(format!(
            "the archive has more than {MAX_ZIP_ENTRIES} entries"
        )));
    }
    let mut sources = BTreeMap::new();
    let mut synctex = Vec::new();
    let mut pdf = Vec::new();
    let mut skipped = Vec::new();
    let mut total = 0u64;
    for i in 0..ar.len() {
        let mut f = ar.by_index(i).map_err(err)?;
        if f.is_dir() {
            continue;
        }
        let Some(path) = f
            .enclosed_name()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
        else {
            skipped.push(format!("{} (unsafe path)", f.name().unwrap_or_default()));
            continue;
        };
        let lower = path.to_ascii_lowercase();
        let wanted = lower.ends_with(".tex")
            || lower.ends_with(".ltx")
            || lower.ends_with(".synctex.gz")
            || lower.ends_with(".synctex")
            || lower.ends_with(".pdf");
        if !wanted
            || path
                .split('/')
                .any(|c| c.starts_with('.') || c == "__MACOSX")
        {
            continue;
        }
        if f.size() > MAX_ENTRY_BYTES {
            skipped.push(format!("{path} (too large)"));
            continue;
        }
        total += f.size();
        if total > MAX_TOTAL_BYTES {
            return Err(err("the archive expands to more than 512 MB"));
        }
        if lower.ends_with(".tex") || lower.ends_with(".ltx") {
            let mut buf = Vec::new();
            (&mut f)
                .take(MAX_ENTRY_BYTES)
                .read_to_end(&mut buf)
                .map_err(err)?;
            match String::from_utf8(buf) {
                Ok(t) => {
                    sources.insert(path, t);
                }
                Err(_) => skipped.push(format!("{path} (not UTF-8)")),
            }
        } else if lower.ends_with(".pdf") {
            pdf.push(path);
        } else {
            synctex.push(path);
        }
    }
    Ok(serde_json::json!({ "sources": sources, "synctex": synctex, "pdf": pdf, "skipped": skipped }).to_string())
}

/// Read one entry of a zip as bytes (bounded).
#[wasm_bindgen]
pub fn zip_entry(bytes: &[u8], name: &str) -> Result<Vec<u8>, JsError> {
    let mut ar = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(err)?;
    let mut f = ar.by_name(name).map_err(err)?;
    let mut buf = Vec::new();
    (&mut f)
        .take(MAX_ENTRY_BYTES)
        .read_to_end(&mut buf)
        .map_err(err)?;
    Ok(buf)
}

fn sources_from_json(json: &str) -> Result<Sources, JsError> {
    let map: BTreeMap<String, String> = serde_json::from_str(json).map_err(err)?;
    for k in map.keys() {
        if k.starts_with('/') || k.split('/').any(|c| c == "..") {
            return Err(err(format!("{k:?} is not a relative path")));
        }
    }
    Ok(Sources::new(map))
}

thread_local! {
    /// PDFium can be bound only once per WebAssembly instance; keep one.
    static PDFIUM: std::cell::OnceCell<Pdfium> = const { std::cell::OnceCell::new() };
}

/// Run `f` with the shared PDFium instance.
fn with_pdfium<T>(f: impl FnOnce(&Pdfium) -> Result<T, JsError>) -> Result<T, JsError> {
    PDFIUM.with(|cell| {
        if cell.get().is_none() {
            let p = proofback_pdf::load_pdfium(None).map_err(err)?;
            let _ = cell.set(p);
        }
        f(cell.get().expect("initialised above"))
    })
}

/// Import an annotated PDF. `sources_json` maps relative paths to file contents.
/// Returns a round file as JSON.
#[wasm_bindgen]
#[allow(clippy::needless_pass_by_value)] // wasm-bindgen needs owned arguments
pub fn import_review(
    pdf: &[u8],
    synctex: Option<Vec<u8>>,
    sources_json: &str,
    round: &str,
    anonymize: bool,
    now: &str,
) -> Result<String, JsError> {
    if !proofback_store::valid_round_name(round) {
        return Err(err(format!("invalid round name {round:?}")));
    }
    let sources = sources_from_json(sources_json)?;
    let extracted = with_pdfium(|p| {
        proofback_pdf::extract(p, pdf, proofback_pdf::Limits::default()).map_err(err)
    })?;
    let st = match &synctex {
        Some(b) if !b.is_empty() => {
            Some(SyncTex::parse(b.as_slice(), proofback_synctex::Limits::default()).map_err(err)?)
        }
        _ => None,
    };
    let texts: HashMap<u32, proofback_pdf::PageText> =
        extracted.texts.into_iter().map(|t| (t.page, t)).collect();
    // The browser has no project root on disk; SyncTeX paths resolve by their
    // `dir/./file` form or relative names.
    let root = std::path::Path::new("/proofback-browser-project");
    let input = Input {
        round,
        annotations: &extracted.annotations,
        boxes: &extracted.boxes,
        texts: &texts,
        synctex: st.as_ref(),
        project_root: root,
        sources: &sources,
        anonymize,
    };
    let items = import(&input);
    let rf = RoundFile::new(
        Round {
            name: round.to_owned(),
            created: now.to_owned(),
            source: Source {
                kind: SourceKind::Pdf,
                file: "review.pdf".into(),
                sha256: proofback_store::sha256_hex(pdf),
                synctex: synctex.as_ref().map(|_| "synctex".to_owned()),
                synctex_sha256: synctex.as_ref().map(|b| proofback_store::sha256_hex(b)),
                commit: None,
            },
            anonymized: anonymize,
        },
        items,
    );
    Ok(rf.to_json())
}

fn round_from(json: &str) -> Result<RoundFile, JsError> {
    RoundFile::from_json(json).map_err(err)
}

/// Unified diff for the given item ids (JSON array), or all exact edit items when empty.
#[wasm_bindgen]
pub fn patch(round_json: &str, sources_json: &str, ids_json: &str) -> Result<String, JsError> {
    let rf = round_from(round_json)?;
    let sources = sources_from_json(sources_json)?;
    let ids: Vec<String> = serde_json::from_str(ids_json).unwrap_or_default();
    let mut edits = Vec::new();
    for it in &rf.items {
        if (!ids.is_empty() && !ids.iter().any(|i| i == it.id.as_str())) || !it.is_edit() {
            continue;
        }
        if let Ok(e) = edit_for(it, &sources) {
            edits.push(e);
        }
    }
    let changes = apply_in_memory(&edits, &sources).map_err(err)?;
    Ok(unified_diff(&changes))
}

/// `markdown`, `letter_markdown`, `letter_latex` or `sarif`.
#[wasm_bindgen]
pub fn export_round(round_json: &str, format: &str) -> Result<String, JsError> {
    let rf = round_from(round_json)?;
    let items: Vec<&Item> = rf.items.iter().collect();
    Ok(match format {
        "markdown" => markdown_checklist(&items),
        "letter_markdown" => letter(&items, "Response to the review", LetterFormat::Markdown),
        "letter_latex" => letter(&items, "Response to the review", LetterFormat::Latex),
        "sarif" => {
            serde_json::to_string_pretty(&sarif(&items, env!("CARGO_PKG_VERSION"))).map_err(err)?
        }
        other => return Err(err(format!("unknown format {other:?}"))),
    })
}

/// The reviewer's PDF with reply notes for answered items.
#[wasm_bindgen]
pub fn answer(pdf: &[u8], round_json: &str, author: &str) -> Result<Vec<u8>, JsError> {
    let rf = round_from(round_json)?;
    let items: Vec<&Item> = rf.items.iter().collect();
    answer_pdf(pdf, &items, author).map(|(b, _)| b).map_err(err)
}

/// Render a page to RGBA pixels. Returns `[width u32 LE, height u32 LE, rgba...]`.
#[wasm_bindgen]
pub fn render_page(pdf: &[u8], page: u32, width: u32) -> Result<Vec<u8>, JsError> {
    with_pdfium(|pdfium| {
        let doc = pdfium.load_pdf_from_byte_slice(pdf, None).map_err(err)?;
        let index = i32::try_from(page.saturating_sub(1)).map_err(err)?;
        let p = doc.pages().get(index).map_err(err)?;
        let cfg = PdfRenderConfig::new()
            .set_target_width(i32::try_from(width.clamp(100, 3000)).unwrap_or(1200))
            .render_form_data(false)
            .render_annotations(true);
        let bmp = p.render_with_config(&cfg).map_err(err)?;
        let (w, h) = (bmp.width() as u32, bmp.height() as u32);
        let mut out = Vec::with_capacity(8 + (w * h * 4) as usize);
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&h.to_le_bytes());
        out.extend_from_slice(&bmp.as_rgba_bytes());
        Ok(out)
    })
}

/// Set an item's status and/or add a reply, with the same guard rails as the CLI.
/// Returns the updated round JSON.
#[wasm_bindgen]
pub fn resolve_item(
    round_json: &str,
    sources_json: &str,
    id: &str,
    status: &str,
    reply: &str,
    author: &str,
    now: &str,
) -> Result<String, JsError> {
    let mut rf = round_from(round_json)?;
    let sources = sources_from_json(sources_json)?;
    let item_id = proofback_model::resolve_prefix(rf.items.iter().map(|i| &i.id), id)
        .map_err(err)?
        .clone();
    let status = if status.is_empty() {
        None
    } else {
        Some(
            proofback_model::Status::parse(status)
                .ok_or_else(|| err(format!("unknown status {status:?}")))?,
        )
    };
    let opts = proofback_store::edit::ResolveOptions {
        status,
        reply: (!reply.trim().is_empty()).then(|| reply.to_owned()),
        author: author.to_owned(),
        now: now.to_owned(),
        force: false,
        commit: None,
    };
    let it = rf.item_mut(&item_id).ok_or_else(|| err("no such item"))?;
    proofback_store::edit::resolve(it, &sources, &opts).map_err(err)?;
    Ok(rf.to_json())
}

/// Page size in PDF points: `[width, height]`.
#[wasm_bindgen]
pub fn page_size(pdf: &[u8], page: u32) -> Result<Vec<f32>, JsError> {
    let (boxes, _) =
        proofback_pdf::read_annotations(pdf, proofback_pdf::Limits::default()).map_err(err)?;
    let b = boxes
        .iter()
        .find(|b| b.page == page)
        .ok_or_else(|| err("no such page"))?;
    Ok(vec![b.width(), b.height()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_reading_is_confined() {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            w.start_file("main.tex", o).unwrap();
            w.write_all(b"hello").unwrap();
            w.start_file("../evil.tex", o).unwrap();
            w.write_all(b"x").unwrap();
            w.start_file("output.synctex.gz", o).unwrap();
            w.write_all(b"x").unwrap();
            w.finish().unwrap();
        }
        let out: serde_json::Value =
            serde_json::from_str(&read_zip(buf.get_ref()).unwrap_or_default()).unwrap();
        assert_eq!(out["sources"]["main.tex"], "hello");
        assert!(out["sources"].get("../evil.tex").is_none());
        assert_eq!(out["synctex"][0], "output.synctex.gz");
    }
}
