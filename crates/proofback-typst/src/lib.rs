//! Typst support.
//!
//! Typst has no SyncTeX, but its compiler knows the source span of every glyph.
//! proofback compiles the project with the embedded Typst compiler and records,
//! for every glyph, its page position and source byte offset. That index plays the
//! role SyncTeX plays for LaTeX, with character precision.
//!
//! Precision guard: the reviewer's PDF may come from another Typst version or other
//! fonts. Every anchor is accepted only when the text under the annotation in the
//! reviewer's PDF matches the glyphs proofback found at that position.

mod world;

use std::collections::HashMap;
use std::ops::Range;

use proofback_anchor::{Sources, classify, make_target};
use proofback_model::{Anchor, AnchorState, Confidence, Item, ItemId, Origin, Reply, Status};
use proofback_pdf::{PageBox, PageText, PdfAnnotation, marked_text};
use proofback_tex::fold_str;
use typst::WorldExt;
use typst::layout::{Abs, Frame, FrameItem, Point, Transform};
use typst_layout::PagedDocument;

pub use world::ProjectWorld;

/// A glyph with its position (PDF points, top-left origin) and source location.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphBox {
    pub page: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub c: char,
    /// All characters of the glyph's cluster (ligatures cover several).
    pub text: String,
    pub file: String,
    /// Byte range in the source that produced the glyph.
    pub source: Range<usize>,
}

/// Compilation output: the document and its glyph index.
pub struct Compiled {
    pub document: PagedDocument,
    pub glyphs: Vec<GlyphBox>,
}

impl std::fmt::Debug for Compiled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Compiled")
            .field("pages", &self.document.pages().len())
            .field("glyphs", &self.glyphs.len())
            .finish()
    }
}

/// Compile the project (`files`: relative path to source text) starting at `main`.
pub fn compile(
    files: &HashMap<String, String>,
    main: &str,
    root: Option<&std::path::Path>,
) -> Result<Compiled, String> {
    let world = ProjectWorld::new(files.clone(), main, root.map(std::path::Path::to_path_buf))?;
    let warned = typst::compile::<PagedDocument>(&world);
    let document = warned.output.map_err(|errs| {
        let msgs: Vec<String> = errs.iter().take(5).map(|e| e.message.to_string()).collect();
        format!("Typst compilation failed: {}", msgs.join("; "))
    })?;
    let mut glyphs = Vec::new();
    for (i, page) in document.pages().iter().enumerate() {
        walk(
            &world,
            &page.frame,
            Transform::identity(),
            i as u32 + 1,
            &mut glyphs,
        );
    }
    Ok(Compiled { document, glyphs })
}

/// PDF of a compiled document (used by tests and by `proofback typst pdf`).
pub fn pdf(c: &Compiled) -> Result<Vec<u8>, String> {
    typst_pdf::pdf(&c.document, &typst_pdf::PdfOptions::default()).map_err(|e| format!("{e:?}"))
}

fn walk(world: &ProjectWorld, frame: &Frame, ts: Transform, page: u32, out: &mut Vec<GlyphBox>) {
    for (pos, item) in frame.items() {
        match item {
            FrameItem::Group(g) => {
                let inner = ts
                    .pre_concat(Transform::translate(pos.x, pos.y))
                    .pre_concat(g.transform);
                walk(world, &g.frame, inner, page, out);
            }
            FrameItem::Text(t) => {
                let size = t.size;
                let m = t.font.metrics();
                let ascent = m.ascender.at(size).to_pt() as f32;
                let descent = -m.descender.at(size).to_pt() as f32;
                let mut x = Abs::zero();
                for g in &t.glyphs {
                    let advance = g.x_advance.at(size);
                    let origin = Point::new(pos.x + x + g.x_offset.at(size), pos.y).transform(ts);
                    let text = t
                        .text
                        .get(g.range.start as usize..g.range.end as usize)
                        .unwrap_or("");
                    let (span, offset) = g.span;
                    let source = span.id().and_then(|id| {
                        let range = world.range(span)?;
                        let file = world.rel_path(id)?;
                        let start = range.start + offset as usize;
                        let len = text.len().max(1);
                        Some((file, start..(start + len).min(range.end.max(start + 1))))
                    });
                    if let (Some((file, source)), Some(c)) = (source, text.chars().next()) {
                        out.push(GlyphBox {
                            page,
                            x: origin.x.to_pt() as f32,
                            y: origin.y.to_pt() as f32 - ascent,
                            w: (advance.to_pt() as f32).max(0.5),
                            h: ascent + descent,
                            c,
                            text: text.to_owned(),
                            file,
                            source,
                        });
                    }
                    x += advance;
                }
            }
            _ => {}
        }
    }
}

/// Anchor annotations on a Typst document.
pub fn import(
    round: &str,
    annotations: &[PdfAnnotation],
    boxes: &[PageBox],
    texts: &HashMap<u32, PageText>,
    compiled: &Compiled,
    sources: &Sources,
    anonymize: bool,
) -> Vec<Item> {
    let boxes: HashMap<u32, &PageBox> = boxes.iter().map(|b| (b.page, b)).collect();
    let mut by_page: HashMap<u32, Vec<&GlyphBox>> = HashMap::new();
    for g in &compiled.glyphs {
        by_page.entry(g.page).or_default().push(g);
    }
    let mut authors: Vec<String> = Vec::new();
    let mut items: Vec<Item> = Vec::new();
    let mut index_of = HashMap::new();
    let mut taken = std::collections::HashSet::new();
    for (ai, a) in annotations
        .iter()
        .enumerate()
        .filter(|(_, a)| a.in_reply_to.is_none())
    {
        let Some(b) = boxes.get(&a.page) else {
            continue;
        };
        let class = classify(a);
        let page_glyphs = by_page.get(&a.page).map_or(&[][..], Vec::as_slice);
        let (target, confidence, reason, marked) =
            anchor_one(a, b, texts.get(&a.page), page_glyphs, sources);
        let id = ItemId::derive_unique(
            round,
            &a.nm.clone().map_or_else(
                || format!("{}:{}:{}", a.page, a.subtype, a.index),
                |nm| format!("nm:{nm}"),
            ),
            |i| taken.contains(i),
        );
        taken.insert(id.clone());
        let author = a.author.as_ref().map(|n| {
            if anonymize {
                let i = authors.iter().position(|x| x == n).unwrap_or_else(|| {
                    authors.push(n.clone());
                    authors.len() - 1
                });
                format!("Reviewer {}", i + 1)
            } else {
                n.clone()
            }
        });
        index_of.insert(ai, items.len());
        items.push(Item {
            id,
            round: round.to_owned(),
            kind: class.kind,
            author,
            created: a.created.clone(),
            body: a.contents.trim().to_owned(),
            suggestion: class.suggestion,
            target,
            anchor: Anchor {
                state: AnchorState::Anchored,
                confidence,
                reason,
                pending: None,
            },
            origin: Origin::Pdf {
                page: a.page,
                subtype: a.subtype.clone(),
                nm: a.nm.clone(),
                quads: a.quads.clone(),
                rect: a.rect,
                marked_text: marked,
                color: a.color.clone(),
            },
            status: Status::Open,
            replies: Vec::new(),
            resolution: proofback_model::Resolution::default(),
            thread: Vec::new(),
        });
    }
    for a in annotations.iter().filter(|a| a.in_reply_to.is_some()) {
        if let Some(&i) = a.in_reply_to.and_then(|p| index_of.get(&p)) {
            items[i].thread.push(Reply {
                author: a.author.clone().unwrap_or_default(),
                body: a.contents.clone(),
                at: a.created.clone().unwrap_or_default(),
            });
        }
    }
    items
}

fn folded(s: &str) -> String {
    fold_str(s.chars())
        .chars
        .into_iter()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn anchor_one(
    a: &PdfAnnotation,
    b: &PageBox,
    text: Option<&PageText>,
    glyphs: &[&GlyphBox],
    sources: &Sources,
) -> (Option<proofback_model::Target>, Confidence, String, String) {
    let point = matches!(a.subtype.as_str(), "Text" | "FreeText" | "Caret");
    // Areas to look in, in top-left coordinates.
    let areas: Vec<[f32; 4]> = if a.quads.is_empty() || point {
        let [x0, y0, x1, y1] = a.rect;
        let (l, t) = b.to_top_left(x0, y1);
        let (r, bt) = b.to_top_left(x1, y0);
        vec![[l, t, r, bt]]
    } else {
        a.quads
            .iter()
            .map(|q| {
                let xs = [q[0], q[2], q[4], q[6]];
                let ys = [q[1], q[3], q[5], q[7]];
                let (l, t) = b.to_top_left(
                    xs.iter().copied().fold(f32::INFINITY, f32::min),
                    ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                );
                let (r, bt) = b.to_top_left(
                    xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                    ys.iter().copied().fold(f32::INFINITY, f32::min),
                );
                [l, t, r, bt]
            })
            .collect()
    };
    let marked = text
        .and_then(|t| marked_text(t, &a.quads, a.rect, 30))
        .map(|m| m.text)
        .unwrap_or_default();
    if a.subtype == "Caret" || point {
        // Insertion point or note: the glyph nearest to the anchor point.
        let (px, py) = if a.subtype == "Caret" {
            b.to_top_left(
                f32::midpoint(a.rect[0], a.rect[2]),
                f32::midpoint(a.rect[1], a.rect[3]),
            )
        } else {
            b.to_top_left(a.rect[0], a.rect[3] - 2.0)
        };
        let near = glyphs
            .iter()
            .filter(|g| py >= g.y - 4.0 && py <= g.y + g.h + 4.0)
            .min_by(|p, q| {
                let dp = (p.x + p.w - px).abs();
                let dq = (q.x + q.w - px).abs();
                dp.total_cmp(&dq)
            });
        let Some(g) = near else {
            return (
                None,
                Confidence::Page,
                "no text near the annotation".into(),
                marked,
            );
        };
        let Some(src) = sources.get(&g.file) else {
            return (
                None,
                Confidence::Page,
                "source file not loaded".into(),
                marked,
            );
        };
        if a.subtype == "Caret" {
            let at = if px >= g.x + g.w / 2.0 {
                g.source.end
            } else {
                g.source.start
            };
            return (
                Some(make_target(&g.file, src, at..at)),
                Confidence::Exact,
                "insertion point from the Typst glyph index".into(),
                marked,
            );
        }
        let (line, _) = src.line_col(g.source.start);
        let r = src.line_range(line as usize).unwrap_or(g.source.clone());
        return (
            Some(make_target(&g.file, src, r)),
            Confidence::Line,
            "note beside this line (Typst glyph index)".into(),
            marked,
        );
    }
    // Glyphs near the annotation, in layout order.
    let near: Vec<&&GlyphBox> = glyphs
        .iter()
        .filter(|g| {
            let cy = g.y + g.h / 2.0;
            areas.iter().any(|r| {
                cy >= r[1] - 0.5 && cy <= r[3] + 0.5 && g.x + g.w >= r[0] - 6.0 && g.x <= r[2] + 6.0
            })
        })
        .collect();
    // Select exactly the glyphs that spell the text the reviewer marked; fall back
    // to geometry when the PDF has no text layer under the annotation.
    let target = folded(&marked);
    let inside: Vec<&&GlyphBox> = if target.is_empty() {
        near.iter()
            .filter(|g| {
                let (cx, cy) = (g.x + g.w / 2.0, g.y + g.h / 2.0);
                areas.iter().any(|r| {
                    cx >= r[0] - 0.5 && cx <= r[2] + 0.5 && cy >= r[1] - 0.5 && cy <= r[3] + 0.5
                })
            })
            .copied()
            .collect()
    } else {
        let mut chars = Vec::new();
        let mut owner = Vec::new();
        for (i, g) in near.iter().enumerate() {
            let mut tmp = Vec::new();
            for ch in g.text.chars() {
                proofback_tex::fold_char(ch, &mut tmp);
            }
            for c in tmp.into_iter().filter(|c| c.is_alphanumeric()) {
                chars.push(c);
                owner.push(i);
            }
        }
        let hay: String = chars.iter().collect();
        let hits: Vec<usize> = hay
            .match_indices(&target)
            .map(|(b, _)| hay[..b].chars().count())
            .collect();
        match hits.as_slice() {
            [h] => {
                let n = target.chars().count();
                let (a, z) = (owner[*h], owner[h + n - 1]);
                near[a..=z].to_vec()
            }
            _ => Vec::new(),
        }
    };
    let Some(first) = inside.first() else {
        return (
            None,
            Confidence::Page,
            "the marked text was not found among the glyphs there".into(),
            marked,
        );
    };
    if inside.iter().any(|g| g.file != first.file) {
        return (
            None,
            Confidence::Page,
            "annotation spans several source files".into(),
            marked,
        );
    }
    let start = inside.iter().map(|g| g.source.start).min().unwrap_or(0);
    let end = inside.iter().map(|g| g.source.end).max().unwrap_or(start);
    let Some(src) = sources.get(&first.file) else {
        return (
            None,
            Confidence::Page,
            "source file not loaded".into(),
            marked,
        );
    };
    let glyph_text: String = inside.iter().map(|g| g.text.as_str()).collect();
    // Precision guard: the reviewer's PDF must show the same text here.
    let agrees = marked.is_empty() || folded(&marked) == folded(&glyph_text);
    if !agrees {
        let (line, _) = src.line_col(start);
        let r = src.line_range(line as usize).unwrap_or(start..end);
        return (
            Some(make_target(&first.file, src, r)),
            Confidence::Line,
            "the reviewer's PDF differs from this build here; check the line".into(),
            marked,
        );
    }
    if end <= start || !src.text.is_char_boundary(start) || !src.text.is_char_boundary(end) {
        return (
            None,
            Confidence::Page,
            "could not map the glyphs to source".into(),
            marked,
        );
    }
    let range = trim_markup(&src.text, start..end);
    (
        Some(make_target(&first.file, src, range)),
        Confidence::Exact,
        "glyphs under the annotation map to this source range (Typst)".into(),
        marked,
    )
}

/// Drop leading or trailing markup characters (`*`, `_`, spaces) from a range.
fn trim_markup(text: &str, r: Range<usize>) -> Range<usize> {
    let s = &text[r.clone()];
    let lead = s.len() - s.trim_start_matches([' ', '*', '_', '\n']).len();
    let trail = s.len() - s.trim_end_matches([' ', '*', '_', '\n']).len();
    if lead + trail >= s.len() {
        return r;
    }
    r.start + lead..r.end - trail
}

#[cfg(test)]
mod tests;
