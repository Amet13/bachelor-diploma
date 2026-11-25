//! The matching pipeline: reviewer annotations in, anchored feedback items out.
//!
//! For each annotation:
//! 1. read the marked text (and context) from the PDF text layer;
//! 2. narrow the search to a few source lines with SyncTeX (when available);
//! 3. align the marked text against the normalised source, treating references
//!    and inline maths as wildcards;
//! 4. map the hit back to an exact source byte range;
//! 5. classify the item and score the confidence honestly.
//!
//! Principle: precision before recall. A hit is only `exact` when it is unique;
//! otherwise the item drops to `line`, `region` or `page`.

pub mod align;
mod classify;
pub mod sources;

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::path::Path;

use proofback_model::{
    Anchor, AnchorState, ByteRange, Confidence, Item, ItemId, Kind, Origin, Quote, Reply, Status,
    Suggestion, Target,
};
use proofback_pdf::{MarkedText, PageBox, PageText, PdfAnnotation, marked_text};
use proofback_synctex::{SyncTex, resolve_input};
use proofback_tex::{BlockKind, fold_str};

pub use classify::{Classified, classify};
pub use sources::{SourceFile, Sources};

/// Characters of PDF context read on each side of the marked text.
const CONTEXT: usize = 40;
/// Characters of source context stored in quote selectors.
const QUOTE_CONTEXT: usize = 32;

/// Everything needed to anchor one review.
#[derive(Debug)]
pub struct Input<'a> {
    pub round: &'a str,
    pub annotations: &'a [PdfAnnotation],
    pub boxes: &'a [PageBox],
    /// Page text for every page that carries annotations.
    pub texts: &'a HashMap<u32, PageText>,
    pub synctex: Option<&'a SyncTex>,
    /// Root used to resolve SyncTeX input paths.
    pub project_root: &'a Path,
    pub sources: &'a Sources,
    pub anonymize: bool,
}

/// Result of anchoring one annotation, before it becomes an [`Item`].
#[derive(Debug, Clone, PartialEq)]
pub struct Anchored {
    pub target: Option<Target>,
    pub confidence: Confidence,
    pub reason: String,
    pub marked: String,
}

/// Anchor every annotation and build items. Replies become threads on their parent.
pub fn import(input: &Input<'_>) -> Vec<Item> {
    let boxes: HashMap<u32, &PageBox> = input.boxes.iter().map(|b| (b.page, b)).collect();
    let mut authors: BTreeMap<String, String> = BTreeMap::new();
    let mut author_name = |a: &Option<String>| -> Option<String> {
        let a = a.as_ref()?.trim();
        if a.is_empty() {
            return None;
        }
        if !input.anonymize {
            return Some(a.to_owned());
        }
        let n = authors.len() + 1;
        Some(
            authors
                .entry(a.to_owned())
                .or_insert_with(|| format!("Reviewer {n}"))
                .clone(),
        )
    };

    let mut items: Vec<Item> = Vec::new();
    let mut index_of: HashMap<usize, usize> = HashMap::new();
    let mut taken: std::collections::HashSet<ItemId> = std::collections::HashSet::new();
    // Parents first, so replies can attach.
    for (ai, a) in input.annotations.iter().enumerate() {
        if a.in_reply_to.is_some() {
            continue;
        }
        let Some(page_box) = boxes.get(&a.page) else {
            continue;
        };
        let text = input.texts.get(&a.page);
        let class = classify(a);
        let anchored = anchor_one(input, a, page_box, text, class.kind);
        let identity = a.nm.clone().map_or_else(
            || format!("{}:{}:{}", a.page, a.subtype, a.index),
            |nm| format!("nm:{nm}"),
        );
        let id = ItemId::derive_unique(input.round, &identity, |id| taken.contains(id));
        taken.insert(id.clone());
        let kind = if anchored.confidence == Confidence::Region
            && matches!(class.kind, Kind::Highlight | Kind::Comment | Kind::General)
        {
            Kind::Figure
        } else {
            class.kind
        };
        index_of.insert(ai, items.len());
        items.push(Item {
            id,
            round: input.round.to_owned(),
            kind,
            author: author_name(&a.author),
            created: a.created.clone(),
            body: a.contents.trim().to_owned(),
            suggestion: class.suggestion,
            target: anchored.target,
            anchor: Anchor {
                state: AnchorState::Anchored,
                confidence: anchored.confidence,
                reason: anchored.reason,
                pending: None,
            },
            origin: Origin::Pdf {
                page: a.page,
                subtype: a.subtype.clone(),
                nm: a.nm.clone(),
                quads: a.quads.iter().map(|q| q.map(round2)).collect(),
                rect: a.rect.map(round2),
                marked_text: anchored.marked,
                color: a.color.clone(),
            },
            status: Status::Open,
            replies: Vec::new(),
            resolution: proofback_model::Resolution::default(),
            thread: Vec::new(),
        });
    }
    for a in input.annotations.iter().filter(|a| a.in_reply_to.is_some()) {
        let mut parent = a.in_reply_to;
        // Follow reply chains to the root.
        for _ in 0..32 {
            match parent
                .and_then(|p| input.annotations.get(p))
                .and_then(|p| p.in_reply_to)
            {
                Some(next) => parent = Some(next),
                None => break,
            }
        }
        if let Some(&idx) = parent.and_then(|p| index_of.get(&p))
            && !a.contents.trim().is_empty()
        {
            items[idx].thread.push(Reply {
                author: author_name(&a.author).unwrap_or_else(|| "unknown".into()),
                body: a.contents.trim().to_owned(),
                at: a.created.clone().unwrap_or_default(),
            });
        }
    }
    items
}

fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

/// SyncTeX candidates mapped to project files: `(file, line, score)`, best first.
fn candidates(input: &Input<'_>, page: u32, points: &[(f32, f32)]) -> Vec<(String, u32, f32)> {
    let Some(st) = input.synctex else {
        return Vec::new();
    };
    let mut tag_files: HashMap<u32, Option<String>> = HashMap::new();
    let mut acc: HashMap<(String, u32), f32> = HashMap::new();
    for &(x, y) in points {
        for c in st.inverse(page, x, y) {
            let file = tag_files
                .entry(c.tag)
                .or_insert_with(|| {
                    let p = st.input(c.tag)?;
                    let rel = resolve_input(input.project_root, p)?;
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    input.sources.get(&rel).map(|_| rel)
                })
                .clone();
            if let Some(file) = file {
                *acc.entry((file, c.line)).or_insert(0.0) += c.score;
            }
        }
    }
    let mut v: Vec<(String, u32, f32)> = acc.into_iter().map(|((f, l), s)| (f, l, s)).collect();
    v.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.0.cmp(&b.0)).then(a.1.cmp(&b.1)));
    v
}

/// Points (in SyncTeX's top-left coordinates) sampled from an annotation.
fn sample_points(a: &PdfAnnotation, b: &PageBox) -> Vec<(f32, f32)> {
    let mut pts = Vec::new();
    if a.quads.is_empty() {
        let [x0, y0, x1, y1] = a.rect;
        pts.push(b.to_top_left(f32::midpoint(x0, x1), f32::midpoint(y0, y1)));
    } else {
        for q in &a.quads {
            let xs = [q[0], q[2], q[4], q[6]];
            let ys = [q[1], q[3], q[5], q[7]];
            let (minx, maxx) = (
                xs.iter().copied().fold(f32::INFINITY, f32::min),
                xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            );
            let (miny, maxy) = (
                ys.iter().copied().fold(f32::INFINITY, f32::min),
                ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            );
            let ym = f32::midpoint(miny, maxy);
            for x in [minx + 1.0, f32::midpoint(minx, maxx), maxx - 1.0] {
                pts.push(b.to_top_left(x, ym));
            }
        }
    }
    pts
}

fn anchor_one(
    input: &Input<'_>,
    a: &PdfAnnotation,
    b: &PageBox,
    text: Option<&PageText>,
    kind: Kind,
) -> Anchored {
    let is_point = matches!(
        a.subtype.as_str(),
        "Text" | "FreeText" | "Caret" | "Stamp" | "FileAttachment" | "Sound"
    );
    // Caret: an insertion point between two characters.
    if a.subtype == "Caret"
        && let Some(t) = text
        && let Some(r) = anchor_caret(input, a, b, t)
    {
        return r;
    }
    let marked: Option<MarkedText> = if is_point {
        text.and_then(|t| nearest_line_text(t, a))
    } else {
        text.and_then(|t| marked_text(t, &a.quads, a.rect, CONTEXT))
    };
    let mut points = sample_points(a, b);
    if is_point && let (Some(t), Some(m)) = (text, &marked) {
        // Anchor point notes on the text line next to them, not the margin.
        if let Some(c) = t.chars.get(m.start) {
            points = vec![b.to_top_left(
                f32::midpoint(c.bbox[0], c.bbox[2]),
                f32::midpoint(c.bbox[1], c.bbox[3]),
            )];
        }
    }
    let cands = candidates(input, a.page, &points);
    let marked_str = marked.as_ref().map(|m| m.text.clone()).unwrap_or_default();

    // Figures, tables and display maths: nothing to align, point at the block.
    let has_text = marked
        .as_ref()
        .is_some_and(|m| m.text.chars().filter(|c| c.is_alphanumeric()).count() >= 2);
    if !has_text || matches!(kind, Kind::Figure | Kind::Math) {
        if let Some(r) = region_from_candidates(input, &cands) {
            return Anchored {
                marked: marked_str,
                ..r
            };
        }
        return fallback(input, &cands, marked_str, "no text under the annotation");
    }
    let m = marked.expect("checked above");
    // For point notes: (focus offset, window length) in PDF characters.
    let point_focus = is_point.then(|| (m.start.saturating_sub(m.end), m.text.chars().count()));
    let found = if cands.is_empty() {
        align_text_only(input, &m)
    } else {
        align_in_windows(input, &m, &cands)
    };
    match found {
        Some((file, range, mut confidence, reason)) => {
            if is_point && confidence == Confidence::Exact {
                // A note beside a line refers to the line, not to exact characters.
                confidence = Confidence::Line;
            }
            let src = input.sources.get(&file).expect("candidate files exist");
            let range = if is_point {
                // The note refers to the line of the character nearest to it.
                let m = point_focus.unwrap_or((0, 1));
                let focus = range.start + (range.end - range.start) * m.0 / m.1.max(1);
                line_range_of(src, focus.min(range.end))
            } else {
                range
            };
            Anchored {
                target: Some(make_target(&file, src, range)),
                confidence,
                reason,
                marked: marked_str,
            }
        }
        None => fallback(
            input,
            &cands,
            marked_str,
            "marked text not found in the source",
        ),
    }
}

/// Characters of the line next to a note, centred on the character nearest to it.
const NOTE_WINDOW: usize = 30;

/// For notes placed next to text: a window of the nearest text line, centred on the
/// character closest to the note. `start..end` is that focus character.
fn nearest_line_text(t: &PageText, a: &PdfAnnotation) -> Option<MarkedText> {
    // Viewers place a note's icon with its top-left corner at the clicked point.
    let cx = a.rect[0];
    let cy = a.rect[3] - 2.0;
    let dist = |c: &proofback_pdf::TextChar| {
        (f32::midpoint(c.bbox[1], c.bbox[3]) - cy).abs() * 4.0
            + (f32::midpoint(c.bbox[0], c.bbox[2]) - cx).abs()
    };
    let best = t
        .chars
        .iter()
        .enumerate()
        .filter(|(_, c)| c.c.is_alphanumeric())
        .min_by(|(_, p), (_, q)| dist(p).total_cmp(&dist(q)))?
        .0;
    let base = t.chars[best].bbox;
    let same_line = |c: &proofback_pdf::TextChar| {
        let mid = f32::midpoint(c.bbox[1], c.bbox[3]);
        mid > base[1] && mid < base[3]
    };
    let mut s = best;
    while s > 0 && best - s < NOTE_WINDOW && same_line(&t.chars[s - 1]) {
        s -= 1;
    }
    let mut e = best + 1;
    while e < t.chars.len() && e - best < NOTE_WINDOW && same_line(&t.chars[e]) {
        e += 1;
    }
    let text: String = t.chars[s..e].iter().map(|c| c.c).collect();
    Some(MarkedText {
        text,
        prefix: t.chars[s.saturating_sub(CONTEXT)..s]
            .iter()
            .map(|c| c.c)
            .collect(),
        suffix: t.chars[e..(e + CONTEXT).min(t.chars.len())]
            .iter()
            .map(|c| c.c)
            .collect(),
        start: best,
        end: s,
    })
}

fn anchor_caret(
    input: &Input<'_>,
    a: &PdfAnnotation,
    b: &PageBox,
    t: &PageText,
) -> Option<Anchored> {
    let cx = a.rect[0].midpoint(a.rect[2]);
    let cy = a.rect[1].midpoint(a.rect[3]);
    // Characters on the caret's line, then the boundary nearest to the caret.
    let line: Vec<usize> = (0..t.chars.len())
        .filter(|&i| {
            let c = &t.chars[i];
            !c.c.is_control() && c.bbox[1] - 3.0 <= a.rect[3] && c.bbox[3] + 3.0 >= a.rect[1]
        })
        .collect();
    let &boundary_char = line.iter().min_by(|&&p, &&q| {
        let dp = (t.chars[p].bbox[2] - cx).abs()
            + (t.chars[p].bbox[1].midpoint(t.chars[p].bbox[3]) - cy).abs();
        let dq = (t.chars[q].bbox[2] - cx).abs()
            + (t.chars[q].bbox[1].midpoint(t.chars[q].bbox[3]) - cy).abs();
        dp.total_cmp(&dq)
    })?;
    // Insert after `boundary_char` when the caret is to its right half.
    let c = &t.chars[boundary_char];
    let split = if cx >= c.bbox[0].midpoint(c.bbox[2]) {
        boundary_char + 1
    } else {
        boundary_char
    };
    let before: String = t.chars[split.saturating_sub(CONTEXT)..split]
        .iter()
        .map(|c| c.c)
        .collect();
    let after: String = t.chars[split..(split + CONTEXT).min(t.chars.len())]
        .iter()
        .map(|c| c.c)
        .collect();
    let fb = fold_str(clean(&before).chars());
    let fa = fold_str(clean(&after).chars());
    let fb_trim: Vec<char> = trim_spaces(&fb.chars);
    let fa_trim: Vec<char> = trim_spaces(&fa.chars);
    if fb_trim.len() < 3 {
        return None;
    }
    let query: Vec<char> = fb_trim
        .iter()
        .chain(std::iter::once(&' '))
        .chain(fa_trim.iter())
        .copied()
        .collect();
    let pts = vec![b.to_top_left(cx, cy)];
    let cands = candidates(input, a.page, &pts);
    let windows = windows_for(input, &cands);
    let mut hits: Vec<(String, usize, usize)> = Vec::new();
    let max_cost = max_cost_for(query.len());
    for (file, win) in &windows {
        let src = input.sources.get(file)?;
        let chars = &src.folded.chars[win.clone()];
        let al = align::align(&query, chars, max_cost);
        for h in al.best {
            // Insertion point: where the text before the caret ends inside the hit.
            let hit = &chars[h.start..h.end];
            let ins = if h.cost == 0 {
                Some(h.start + fb_trim.len())
            } else {
                let b = align::align(&fb_trim, hit, max_cost_for(fb_trim.len()));
                (b.best.len() == 1).then(|| h.start + b.best[0].end)
            };
            if let Some(ins) = ins {
                hits.push((file.clone(), win.start + ins, h.cost));
            }
        }
    }
    let marked = format!("{}‸{}", before.trim(), after.trim());
    if hits.len() != 1 {
        return None;
    }
    let (file, ins, _) = hits.remove(0);
    let src = input.sources.get(&file)?;
    let byte = src.source_range_for_folded(ins - 1..ins)?.end;
    Some(Anchored {
        target: Some(make_target(&file, src, byte..byte)),
        confidence: Confidence::Exact,
        reason: "insertion point: unique match of the text around the caret".into(),
        marked,
    })
}

fn trim_spaces(v: &[char]) -> Vec<char> {
    let s = v.iter().position(|c| *c != ' ').unwrap_or(v.len());
    let e = v.iter().rposition(|c| *c != ' ').map_or(s, |e| e + 1);
    v[s..e].to_vec()
}

fn clean(s: &str) -> String {
    s.replace("-\r\n", "")
        .replace("-\n", "")
        .replace(['\r', '\n', '\u{2}'], " ")
}

/// Search windows (folded char ranges) around SyncTeX candidates, per file.
fn windows_for(input: &Input<'_>, cands: &[(String, u32, f32)]) -> Vec<(String, Range<usize>)> {
    let Some(top) = cands.first().map(|c| c.2) else {
        return Vec::new();
    };
    let mut per_file: BTreeMap<&str, (u32, u32, f32)> = BTreeMap::new();
    for (f, l, s) in cands {
        if *s < top * 0.15 {
            continue;
        }
        let e = per_file.entry(f.as_str()).or_insert((*l, *l, 0.0));
        e.0 = e.0.min(*l);
        e.1 = e.1.max(*l);
        e.2 += *s;
    }
    let mut files: Vec<(&str, (u32, u32, f32))> = per_file.into_iter().collect();
    files.sort_by(|a, b| b.1.2.total_cmp(&a.1.2));
    files
        .into_iter()
        .take(2)
        .filter_map(|(f, (lo, hi, _))| {
            let src = input.sources.get(f)?;
            let lo = (lo as usize).saturating_sub(4).max(1);
            let hi = (hi as usize + 4).min(src.line_count());
            let start = src.line_range(lo)?.start;
            let end = src.line_range(hi)?.end;
            let r = src.folded_range_for_source(start..end)?;
            Some((f.to_owned(), r))
        })
        .collect()
}

type Found = (String, Range<usize>, Confidence, String);

fn max_cost_for(len: usize) -> usize {
    if len < 6 { 0 } else { (len * 12 / 100).max(1) }
}

fn align_in_windows(
    input: &Input<'_>,
    m: &MarkedText,
    cands: &[(String, u32, f32)],
) -> Option<Found> {
    let q = fold_str(m.text.chars());
    let query = trim_spaces(&q.chars);
    if query.is_empty() {
        return None;
    }
    let pre = trim_spaces(&fold_str(m.prefix.chars()).chars);
    let suf = trim_spaces(&fold_str(m.suffix.chars()).chars);
    let windows = windows_for(input, cands);
    let max_cost = max_cost_for(query.len());
    let mut all: Vec<(String, Range<usize>, usize)> = Vec::new();
    let mut second: Option<usize> = None;
    for (file, win) in &windows {
        let src = input.sources.get(file)?;
        let al = align::align(&query, &src.folded.chars[win.clone()], max_cost);
        second = match (second, al.second_cost) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        for h in al.best {
            all.push((file.clone(), win.start + h.start..win.start + h.end, h.cost));
        }
    }
    let min = all.iter().map(|h| h.2).min()?;
    all.retain(|h| h.2 == min);
    let top_line = cands.first().map(|c| (c.0.clone(), c.1));
    choose(input, all, min, second, &pre, &suf, top_line, query.len())
}

#[allow(clippy::too_many_arguments)]
fn choose(
    input: &Input<'_>,
    mut hits: Vec<(String, Range<usize>, usize)>,
    cost: usize,
    second: Option<usize>,
    pre: &[char],
    suf: &[char],
    top_line: Option<(String, u32)>,
    qlen: usize,
) -> Option<Found> {
    if hits.is_empty() {
        return None;
    }
    let fuzzy_ok = cost == 0 || second.is_none_or(|s| s > cost + 1);
    if hits.len() == 1 {
        let (f, r, _) = hits.remove(0);
        let src = input.sources.get(&f)?;
        let range = src.source_range_for_folded(r)?;
        let (conf, reason) = if fuzzy_ok {
            (
                Confidence::Exact,
                if cost == 0 {
                    "unique match of the marked text".to_owned()
                } else {
                    format!("unique close match ({cost} edits)")
                },
            )
        } else {
            (
                Confidence::Line,
                "close match with competing alternatives".to_owned(),
            )
        };
        return Some((f, range, conf, reason));
    }
    // Several equally good hits: use the PDF context to choose.
    let mut scored: Vec<(usize, String, Range<usize>)> = hits
        .into_iter()
        .map(|(f, r, _)| {
            let src = input.sources.get(&f).expect("hit files exist");
            let before = &src.folded.chars[..r.start];
            let after = &src.folded.chars[r.end..];
            let mut s = 0;
            for (p, t) in pre
                .iter()
                .rev()
                .zip(before.iter().rev().skip_while(|c| **c == ' '))
            {
                if p == t {
                    s += 1;
                } else {
                    break;
                }
            }
            for (p, t) in suf.iter().zip(after.iter().skip_while(|c| **c == ' ')) {
                if p == t {
                    s += 1;
                } else {
                    break;
                }
            }
            (s, f, r)
        })
        .collect();
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    let (best_s, f, r) = scored[0].clone();
    let runner_up = scored[1].0;
    let src = input.sources.get(&f)?;
    if best_s >= runner_up + 3 && cost == 0 {
        let range = src.source_range_for_folded(r)?;
        return Some((
            f,
            range,
            Confidence::Exact,
            "unique match once the surrounding text is considered".into(),
        ));
    }
    // Still ambiguous: prefer the hit on the best SyncTeX line, but only claim the line.
    let pick = top_line
        .and_then(|(tf, tl)| {
            scored.iter().find(|(_, f2, r2)| {
                f2 == &tf
                    && input
                        .sources
                        .get(f2)
                        .and_then(|s| s.source_range_for_folded(r2.clone()))
                        .is_some_and(|br| {
                            let (l, _) = input.sources.get(f2).expect("exists").line_col(br.start);
                            l == tl
                        })
            })
        })
        .cloned()
        .unwrap_or_else(|| scored[0].clone());
    let src = input.sources.get(&pick.1)?;
    let range = src.source_range_for_folded(pick.2)?;
    let _ = qlen;
    Some((
        pick.1,
        range,
        Confidence::Line,
        format!(
            "{} equally good matches; kept the one SyncTeX points at",
            scored.len()
        ),
    ))
}

fn align_text_only(input: &Input<'_>, m: &MarkedText) -> Option<Found> {
    let query: String = trim_spaces(&fold_str(m.text.chars()).chars)
        .into_iter()
        .collect();
    if query.chars().count() < 3 {
        return None;
    }
    let pre = trim_spaces(&fold_str(m.prefix.chars()).chars);
    let suf = trim_spaces(&fold_str(m.suffix.chars()).chars);
    let mut hits = Vec::new();
    for (f, src) in input.sources.files() {
        for r in src.find_folded(&query) {
            hits.push((f.to_owned(), r, 0));
        }
    }
    let n = hits.len();
    let qlen = query.chars().count();
    let mut found = choose(input, hits, 0, None, &pre, &suf, None, qlen)?;
    // Without SyncTeX, short unique matches are only trusted to the line.
    if found.2 == Confidence::Exact && n == 1 && qlen < 20 {
        found.2 = Confidence::Line;
        found.3 = "unique match, but short and found without SyncTeX".into();
    } else if found.2 == Confidence::Exact {
        found.3 = format!("{} (without SyncTeX)", found.3);
    }
    Some(found)
}

fn region_from_candidates(input: &Input<'_>, cands: &[(String, u32, f32)]) -> Option<Anchored> {
    for (f, line, _) in cands.iter().take(5) {
        let src = input.sources.get(f)?;
        let lr = src.line_range(*line as usize)?;
        if let Some(block) = src.normalized.blocks().iter().find(|b| {
            matches!(
                b.kind,
                BlockKind::Figure | BlockKind::Table | BlockKind::DisplayMath
            ) && b.source.start <= lr.start
                && lr.end <= b.source.end
        }) {
            let what = match block.kind {
                BlockKind::Figure => "figure",
                BlockKind::Table => "table",
                _ => "display maths",
            };
            return Some(Anchored {
                target: Some(make_target(f, src, block.source.clone())),
                confidence: Confidence::Region,
                reason: format!("annotation is on a {what}"),
                marked: String::new(),
            });
        }
    }
    None
}

fn fallback(
    input: &Input<'_>,
    cands: &[(String, u32, f32)],
    marked: String,
    why: &str,
) -> Anchored {
    if let Some((f, line, _)) = cands.first()
        && let Some(src) = input.sources.get(f)
        && let Some(r) = src.line_range(*line as usize)
    {
        return Anchored {
            target: Some(make_target(f, src, trim_range(&src.text, r))),
            confidence: Confidence::Line,
            reason: format!("{why}; SyncTeX line"),
            marked,
        };
    }
    Anchored {
        target: None,
        confidence: Confidence::Page,
        reason: format!("{why}; needs a human"),
        marked,
    }
}

fn trim_range(text: &str, r: Range<usize>) -> Range<usize> {
    let s = &text[r.clone()];
    let lead = s.len() - s.trim_start().len();
    let trail = s.len() - s.trim_end().len();
    if lead + trail >= s.len() {
        return r;
    }
    r.start + lead..r.end - trail
}

fn line_range_of(src: &SourceFile, offset: usize) -> Range<usize> {
    let (line, _) = src.line_col(offset);
    src.line_range(line as usize)
        .map_or(offset..offset, |r| trim_range(&src.text, r))
}

/// Build a [`Target`] with line/column positions and quote context.
pub fn make_target(file: &str, src: &SourceFile, range: Range<usize>) -> Target {
    let (line, column) = src.line_col(range.start);
    let (end_line, end_column) = src.line_col(range.end);
    let text = &src.text;
    let prefix_start = floor_char(text, range.start.saturating_sub(QUOTE_CONTEXT * 2));
    let prefix: String = text[prefix_start..range.start]
        .chars()
        .rev()
        .take(QUOTE_CONTEXT)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let suffix_end = ceil_char(text, (range.end + QUOTE_CONTEXT * 4).min(text.len()));
    let suffix: String = text[range.end..suffix_end]
        .chars()
        .take(QUOTE_CONTEXT)
        .collect();
    Target {
        file: file.to_owned(),
        range: ByteRange {
            start: range.start,
            end: range.end,
        },
        line,
        column,
        end_line,
        end_column,
        quote: Quote {
            exact: text[range].to_owned(),
            prefix,
            suffix,
        },
    }
}

fn floor_char(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_char(s: &str, mut i: usize) -> usize {
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Build a [`Suggestion`] if any (re-exported for other crates).
pub fn suggestion_for(kind: Kind, contents: &str) -> Option<Suggestion> {
    classify::suggestion(kind, contents)
}
