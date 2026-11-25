//! Positioned page text from PDFium.

use std::path::{Path, PathBuf};

use pdfium_render::prelude::*;

use crate::Error;

/// One character with its loose bounding box `[left, bottom, right, top]`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextChar {
    pub c: char,
    pub bbox: [f32; 4],
}

impl TextChar {
    fn center(&self) -> (f32, f32) {
        (
            f32::midpoint(self.bbox[0], self.bbox[2]),
            f32::midpoint(self.bbox[1], self.bbox[3]),
        )
    }
}

/// All characters of a page in reading order, as PDFium produced them.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PageText {
    /// 1-based page number.
    pub page: u32,
    pub chars: Vec<TextChar>,
}

impl PageText {
    pub fn text(&self) -> String {
        self.chars.iter().map(|c| c.c).collect()
    }
}

/// Bind PDFium. Looks at `hint`, then `PROOFBACK_PDFIUM`, then next to the running
/// executable, then the workspace's `target/pdfium`, then the system library path.
pub fn load_pdfium(hint: Option<&Path>) -> Result<Pdfium, Error> {
    let mut tried = Vec::new();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(h) = hint {
        candidates.push(h.to_path_buf());
    }
    if let Some(env) = std::env::var_os("PROOFBACK_PDFIUM") {
        candidates.push(PathBuf::from(env));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.to_path_buf());
            candidates.push(dir.join("lib"));
            candidates.push(dir.join("../lib"));
        }
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/pdfium");
    candidates.push(workspace.join("lib"));
    candidates.push(workspace.join("bin"));
    for c in candidates {
        let path = if c.is_dir() {
            Pdfium::pdfium_platform_library_name_at_path(&c)
        } else {
            c.clone()
        };
        if path.is_file() {
            match Pdfium::bind_to_library(&path) {
                Ok(b) => return Ok(Pdfium::new(b)),
                Err(e) => tried.push(format!("{}: {e}", path.display())),
            }
        }
    }
    match Pdfium::bind_to_system_library() {
        Ok(b) => Ok(Pdfium::new(b)),
        Err(e) => {
            tried.push(format!("system library: {e}"));
            Err(Error::PdfiumMissing(tried.join("; ")))
        }
    }
}

/// Extracts positioned text from a loaded document.
pub struct TextExtractor<'a> {
    doc: PdfDocument<'a>,
}

impl std::fmt::Debug for TextExtractor<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextExtractor").finish_non_exhaustive()
    }
}

impl<'a> TextExtractor<'a> {
    pub fn open(pdfium: &'a Pdfium, bytes: &'a [u8]) -> Result<Self, Error> {
        let doc = pdfium
            .load_pdf_from_byte_slice(bytes, None)
            .map_err(|e| match e {
                PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
                    Error::Encrypted
                }
                other => Error::Pdfium(other.to_string()),
            })?;
        Ok(Self { doc })
    }

    pub fn page_count(&self) -> u32 {
        u32::try_from(self.doc.pages().len()).unwrap_or(0)
    }

    /// Characters of one page (1-based).
    pub fn page(&self, number: u32) -> Result<PageText, Error> {
        let index = i32::try_from(number.saturating_sub(1))
            .map_err(|_| Error::Pdfium("page out of range".into()))?;
        let page = self
            .doc
            .pages()
            .get(index)
            .map_err(|e| Error::Pdfium(e.to_string()))?;
        let text = page.text().map_err(|e| Error::Pdfium(e.to_string()))?;
        let mut chars = Vec::with_capacity(usize::try_from(text.len()).unwrap_or(0));
        for ch in text.chars().iter() {
            let Some(c) = ch.unicode_char() else { continue };
            let Ok(r) = ch.loose_bounds() else { continue };
            chars.push(TextChar {
                c,
                bbox: [
                    r.left().value,
                    r.bottom().value,
                    r.right().value,
                    r.top().value,
                ],
            });
        }
        Ok(PageText {
            page: number,
            chars,
        })
    }
}

/// The text under a markup annotation, with context.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MarkedText {
    pub text: String,
    pub prefix: String,
    pub suffix: String,
    /// Character indices into [`PageText::chars`].
    pub start: usize,
    pub end: usize,
}

/// Characters whose centre lies inside any of the quads (or the rect when there are
/// no quads), plus up to `context` characters before and after in reading order.
pub fn marked_text(
    page: &PageText,
    quads: &[[f32; 8]],
    rect: [f32; 4],
    context: usize,
) -> Option<MarkedText> {
    let boxes: Vec<[f32; 4]> = if quads.is_empty() {
        vec![rect]
    } else {
        quads
            .iter()
            .map(|q| {
                let xs = [q[0], q[2], q[4], q[6]];
                let ys = [q[1], q[3], q[5], q[7]];
                [
                    xs.iter().copied().fold(f32::INFINITY, f32::min),
                    ys.iter().copied().fold(f32::INFINITY, f32::min),
                    xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                    ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                ]
            })
            .collect()
    };
    let inside = |c: &TextChar| {
        let (x, y) = c.center();
        boxes
            .iter()
            .any(|b| x >= b[0] - 0.5 && x <= b[2] + 0.5 && y >= b[1] - 0.5 && y <= b[3] + 0.5)
    };
    let first = page
        .chars
        .iter()
        .position(|c| !c.c.is_whitespace() && inside(c))?;
    let last = page
        .chars
        .iter()
        .rposition(|c| !c.c.is_whitespace() && inside(c))?;
    let text: String = page.chars[first..=last].iter().map(|c| c.c).collect();
    let pre_start = first.saturating_sub(context);
    let post_end = (last + 1 + context).min(page.chars.len());
    Some(MarkedText {
        text: clean(&text),
        prefix: clean(
            &page.chars[pre_start..first]
                .iter()
                .map(|c| c.c)
                .collect::<String>(),
        ),
        suffix: clean(
            &page.chars[last + 1..post_end]
                .iter()
                .map(|c| c.c)
                .collect::<String>(),
        ),
        start: first,
        end: last + 1,
    })
}

/// Undo line-end hyphenation and normalise line breaks to spaces.
fn clean(s: &str) -> String {
    let s = s
        .replace("-\r\n", "")
        .replace("-\n", "")
        .replace('\u{2}', "");
    s.replace("\r\n", " ").replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(s: &str) -> PageText {
        let chars = s
            .chars()
            .enumerate()
            .map(|(i, c)| TextChar {
                c,
                bbox: [i as f32 * 10.0, 0.0, i as f32 * 10.0 + 9.0, 10.0],
            })
            .collect();
        PageText { page: 1, chars }
    }

    #[test]
    fn marked_text_reads_under_quads() {
        let p = page("The estimator is strong under noise");
        // "strong" occupies characters 17..23.
        let q = [170.0, 10.0, 229.0, 10.0, 170.0, 0.0, 229.0, 0.0];
        let m = marked_text(&p, &[q], [0.0; 4], 5).unwrap();
        assert_eq!(m.text, "strong");
        assert_eq!(m.prefix, "r is ");
        assert_eq!(m.suffix, " unde");
        assert!(
            marked_text(
                &p,
                &[[900.0, 10.0, 950.0, 10.0, 900.0, 0.0, 950.0, 0.0]],
                [0.0; 4],
                5
            )
            .is_none()
        );
    }

    #[test]
    fn hyphenation_is_undone() {
        assert_eq!(clean("hyphen-\nated words"), "hyphenated words");
    }
}
