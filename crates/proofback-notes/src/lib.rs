//! Free-text review notes ("p. 12, para 2: unclear", "3.1: 'state of the art' ->
//! 'current'") turned into anchored items.
//!
//! Parsing is deterministic. Location uses, in order: a quoted phrase (text
//! alignment, disambiguated by page or section), a section number (document
//! numbering computed from the sources), and a page plus paragraph (SyncTeX).
//! Notes structured by an AI model (`parse_structured`) start at `page` confidence
//! until the author confirms them.

mod sections;

use std::sync::LazyLock;

use proofback_anchor::{Sources, make_target};
use proofback_model::{
    Anchor, AnchorState, Confidence, Item, ItemId, Kind, Origin, Status, Suggestion, Target,
};
use proofback_synctex::{SyncTex, resolve_input};
use proofback_tex::fold_str;
use regex::Regex;
use serde::{Deserialize, Serialize};

pub use sections::{Heading, headings};

/// One parsed note.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// 1-based line in the notes file.
    pub line: u32,
    pub text: String,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub paragraph: Option<u32>,
    #[serde(default)]
    pub section: Option<String>,
    /// Phrase from the document the note refers to.
    #[serde(default)]
    pub quote: Option<String>,
    /// Replacement for `quote`.
    #[serde(default)]
    pub replacement: Option<String>,
    /// Text to insert after `quote`.
    #[serde(default)]
    pub insert: Option<String>,
    #[serde(default)]
    pub delete: bool,
}

static PAGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:pp?|pg|pages?)\.?\s*(\d{1,5})").expect("regex"));
static PARA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:¶|\bpara(?:graph)?\b\.?|\bpar\.)\s*(\d{1,3})").expect("regex")
});
static SECTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:§|\bsec(?:tion)?\b\.?|\bchapter\b|\bch\.)\s*(\d{1,3}(?:\.\d{1,3}){0,3})|^\s*(\d{1,3}(?:\.\d{1,3}){1,3})\s*[:)]").expect("regex")
});
static QUOTED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"“([^”]{2,300})”|"([^"]{2,300})"|‘([^’]{2,300})’|'([^']{2,300})'|«([^»]{2,300})»"#)
        .expect("regex")
});
static ARROW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s*(?:->|→|=>|⇒|\bshould be\b|\bto\b|\bwith\b)\s*$").expect("regex")
});
static DELETE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:delete|remove|drop|cut|strike)\b").expect("regex"));
static INSERT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:insert|add)\b").expect("regex"));

/// Parse a notes file: one note per line or bullet; indented lines continue the
/// previous note. Quoted email lines (`>`) are skipped.
pub fn parse(text: &str) -> Vec<Note> {
    let mut notes: Vec<Note> = Vec::new();
    for (i, raw) in text.lines().enumerate().take(20_000) {
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('>') {
            continue;
        }
        let continuation =
            raw.starts_with([' ', '\t']) && !is_bullet(line.trim_start()) && !notes.is_empty();
        let body = strip_bullet(line.trim())
            .chars()
            .take(2000)
            .collect::<String>();
        if continuation {
            let last = notes.last_mut().expect("checked");
            last.text.push(' ');
            last.text.push_str(&body);
            continue;
        }
        notes.push(Note {
            line: i as u32 + 1,
            text: body,
            ..Note::default()
        });
    }
    for n in &mut notes {
        fill(n);
    }
    notes
}

fn is_bullet(s: &str) -> bool {
    s.starts_with(['-', '*', '•'])
        || s.split_once(['.', ')']).is_some_and(|(a, _)| {
            !a.is_empty() && a.len() <= 3 && a.bytes().all(|b| b.is_ascii_digit())
        })
}

fn strip_bullet(s: &str) -> &str {
    if let Some(r) = s.strip_prefix(['-', '*', '•']) {
        return r.trim_start();
    }
    if let Some((a, r)) = s.split_once(['.', ')'])
        && !a.is_empty()
        && a.len() <= 3
        && a.bytes().all(|b| b.is_ascii_digit())
        && r.starts_with(' ')
    {
        return r.trim_start();
    }
    s
}

/// Extract page, paragraph, section, quotes and the requested change.
fn fill(n: &mut Note) {
    let t = n.text.clone();
    n.page = PAGE.captures(&t).and_then(|c| c[1].parse().ok());
    n.paragraph = PARA.captures(&t).and_then(|c| c[1].parse().ok());
    n.section = SECTION
        .captures(&t)
        .and_then(|c| c.get(1).or_else(|| c.get(2)).map(|m| m.as_str().to_owned()));
    let quotes: Vec<(usize, usize, String)> = QUOTED
        .captures_iter(&t)
        .filter_map(|c| {
            let m = (1..=5).find_map(|i| c.get(i))?;
            let whole = c.get(0)?;
            Some((whole.start(), whole.end(), m.as_str().trim().to_owned()))
        })
        .collect();
    match quotes.as_slice() {
        [] => {}
        [(_, _, q)] => {
            n.quote = Some(q.clone());
            n.delete = DELETE.is_match(&t);
        }
        [(_, e1, q1), (s2, _, q2), ..] => {
            let between = &t[*e1..*s2];
            if ARROW.is_match(between) || between.trim().is_empty() {
                n.quote = Some(q1.clone());
                n.replacement = Some(q2.clone());
            } else if INSERT.is_match(&t[..*e1]) && between.to_lowercase().contains("after") {
                n.insert = Some(q1.clone());
                n.quote = Some(q2.clone());
            } else {
                n.quote = Some(q1.clone());
            }
        }
    }
}

/// Notes structured by a model, as JSON (see `proofback-ai`). Values are bounded and
/// validated; anything else is ignored.
pub fn parse_structured(json: &str) -> Result<Vec<Note>, String> {
    let v: Vec<Note> =
        serde_json::from_str(json).map_err(|e| format!("model returned invalid notes: {e}"))?;
    Ok(v.into_iter()
        .take(5000)
        .map(|mut n| {
            let clip = |s: Option<String>| {
                s.map(|s| s.chars().take(300).collect::<String>())
                    .filter(|s| !s.trim().is_empty())
            };
            n.text = n.text.chars().take(2000).collect();
            n.quote = clip(n.quote);
            n.replacement = clip(n.replacement);
            n.insert = clip(n.insert);
            n.section = n
                .section
                .filter(|s| s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit() || b == b'.'));
            n.page = n.page.filter(|p| *p > 0 && *p < 100_000);
            n.paragraph = n.paragraph.filter(|p| *p > 0 && *p < 1000);
            n
        })
        .collect())
}

/// Where to look.
#[derive(Debug)]
pub struct Context<'a> {
    pub sources: &'a Sources,
    pub synctex: Option<&'a SyncTex>,
    pub project_root: &'a std::path::Path,
    /// Main file, for section numbering.
    pub main: &'a str,
}

/// Turn notes into items. `ai_parsed` caps confidence at `page` until confirmed.
pub fn locate(notes: &[Note], ctx: &Context<'_>, round: &str, ai_parsed: bool) -> Vec<Item> {
    let heads = headings(ctx.sources, ctx.main);
    let mut taken = std::collections::HashSet::new();
    notes
        .iter()
        .map(|n| {
            let (target, mut confidence, mut reason) = locate_one(n, ctx, &heads);
            let mut pending = None;
            if ai_parsed && confidence != Confidence::Page {
                pending = Some(confidence);
                reason = format!(
                    "structured by an AI model; confirm with `proofback confirm` ({reason})"
                );
                confidence = Confidence::Page;
            }
            let kind = if n.replacement.is_some() {
                Kind::Replace
            } else if n.insert.is_some() {
                Kind::Insert
            } else if n.delete && n.quote.is_some() {
                Kind::Delete
            } else if n.text.contains('?') {
                Kind::Question
            } else if n.quote.is_some() || n.page.is_some() || n.section.is_some() {
                Kind::Comment
            } else {
                Kind::General
            };
            let suggestion = match kind {
                Kind::Replace => n.replacement.clone().map(|r| Suggestion {
                    replace_with: Some(r),
                    insert: None,
                }),
                Kind::Insert => n.insert.clone().map(|i| Suggestion {
                    replace_with: None,
                    insert: Some(i),
                }),
                _ => None,
            };
            // An insertion goes after the quoted phrase.
            let target = match (kind, target) {
                (Kind::Insert, Some(t)) => ctx
                    .sources
                    .get(&t.file)
                    .map(|src| make_target(&t.file, src, t.range.end..t.range.end)),
                (_, t) => t,
            };
            let id = ItemId::derive_unique(round, &format!("note:{}:{}", n.line, n.text), |i| {
                taken.contains(i)
            });
            taken.insert(id.clone());
            Item {
                id,
                round: round.to_owned(),
                kind,
                author: None,
                created: None,
                body: n.text.clone(),
                suggestion,
                target,
                anchor: Anchor {
                    state: AnchorState::Anchored,
                    confidence,
                    reason,
                    pending,
                },
                origin: Origin::Notes {
                    line: n.line,
                    text: n.text.clone(),
                    page: n.page,
                },
                status: Status::Open,
                replies: Vec::new(),
                resolution: proofback_model::Resolution::default(),
                thread: Vec::new(),
            }
        })
        .collect()
}

type Located = (Option<Target>, Confidence, String);

fn locate_one(n: &Note, ctx: &Context<'_>, heads: &[Heading]) -> Located {
    let section = n
        .section
        .as_deref()
        .and_then(|s| heads.iter().position(|h| h.number == s));
    let section_range = section.map(|i| sections::range_of(heads, i, ctx.sources));
    if let Some(q) = n.quote.as_deref() {
        let folded: String = fold_str(q.chars())
            .chars
            .iter()
            .collect::<String>()
            .trim()
            .to_owned();
        if folded.chars().count() >= 3 {
            let mut hits: Vec<(String, std::ops::Range<usize>)> = Vec::new();
            for (f, src) in ctx.sources.files() {
                for r in src.find_folded(&folded) {
                    if let Some(br) = src.source_range_for_folded(r) {
                        hits.push((f.to_owned(), br));
                    }
                }
            }
            let total = hits.len();
            if let Some((file, sr)) = &section_range {
                let narrowed: Vec<_> = hits
                    .iter()
                    .filter(|(f, r)| f == file && r.start >= sr.start && r.end <= sr.end)
                    .cloned()
                    .collect();
                if !narrowed.is_empty() {
                    hits = narrowed;
                }
            }
            if hits.len() > 1
                && let (Some(page), Some(st)) = (n.page, ctx.synctex)
            {
                let on_page: Vec<_> = hits
                    .iter()
                    .filter(|(f, r)| line_on_page(ctx, st, f, r.start, page))
                    .cloned()
                    .collect();
                if !on_page.is_empty() {
                    hits = on_page;
                }
            }
            match hits.as_slice() {
                [(f, r)] => {
                    let src = ctx.sources.get(f).expect("hit file");
                    let why = if total == 1 {
                        "unique match of the quoted text".to_owned()
                    } else {
                        format!("quoted text found {total} times; narrowed by page or section")
                    };
                    return (Some(make_target(f, src, r.clone())), Confidence::Exact, why);
                }
                [] => {}
                many => {
                    return (
                        None,
                        Confidence::Page,
                        format!(
                            "quoted text found {} times; add a page or section to the note",
                            many.len()
                        ),
                    );
                }
            }
        }
    }
    if let (Some(i), Some((file, range))) = (section, section_range) {
        let src = ctx.sources.get(&file).expect("heading file");
        if let Some(p) = n.paragraph
            && let Some(pr) = nth_paragraph(&src.text, range.clone(), p as usize)
        {
            return (
                Some(make_target(&file, src, pr)),
                Confidence::Line,
                format!("paragraph {p} of section {}", heads[i].number),
            );
        }
        return (
            Some(make_target(&file, src, range)),
            Confidence::Region,
            format!("section {} ({})", heads[i].number, heads[i].title),
        );
    }
    if let (Some(page), Some(st)) = (n.page, ctx.synctex)
        && let Some((file, pr)) =
            paragraph_on_page(ctx, st, page, n.paragraph.unwrap_or(1) as usize)
    {
        let src = ctx.sources.get(&file).expect("page file");
        let conf = if n.paragraph.is_some() {
            Confidence::Line
        } else {
            Confidence::Page
        };
        let why = match n.paragraph {
            Some(p) => format!("paragraph {p} on page {page} (SyncTeX)"),
            None => format!("page {page}; no paragraph given"),
        };
        let target = (conf == Confidence::Line).then(|| make_target(&file, src, pr));
        return (target, conf, why);
    }
    let why = if n.page.is_some() {
        "page only; build with SyncTeX for paragraph-level locations"
    } else {
        "no location in the note"
    };
    (None, Confidence::Page, why.to_owned())
}

/// Source lines of `file` that SyncTeX places on `page`.
fn lines_on_page(ctx: &Context<'_>, st: &SyncTex, page: u32) -> Vec<(String, u32, f32)> {
    let Some(p) = st.page(page) else {
        return Vec::new();
    };
    let mut out: Vec<(String, u32, f32)> = Vec::new();
    let mut tags: std::collections::HashMap<u32, Option<String>> = std::collections::HashMap::new();
    for r in &p.records {
        let f = tags
            .entry(r.tag)
            .or_insert_with(|| {
                let rel = resolve_input(ctx.project_root, st.input(r.tag)?)?;
                let rel = rel.to_string_lossy().replace('\\', "/");
                ctx.sources.get(&rel).map(|_| rel)
            })
            .clone();
        if let Some(f) = f
            && !out.iter().any(|(ff, l, _)| ff == &f && *l == r.line)
        {
            out.push((f, r.line, r.y));
        }
    }
    out.sort_by(|a, b| a.2.total_cmp(&b.2));
    out
}

fn line_on_page(ctx: &Context<'_>, st: &SyncTex, file: &str, offset: usize, page: u32) -> bool {
    let Some(src) = ctx.sources.get(file) else {
        return false;
    };
    let (line, _) = src.line_col(offset);
    lines_on_page(ctx, st, page)
        .iter()
        .any(|(f, l, _)| f == file && *l == line)
}

/// The `k`-th source paragraph that appears on `page`, in reading order.
fn paragraph_on_page(
    ctx: &Context<'_>,
    st: &SyncTex,
    page: u32,
    k: usize,
) -> Option<(String, std::ops::Range<usize>)> {
    let mut seen: Vec<(String, std::ops::Range<usize>)> = Vec::new();
    for (f, l, _) in lines_on_page(ctx, st, page) {
        let src = ctx.sources.get(&f)?;
        let lr = src.line_range(l as usize)?;
        let para = paragraph_around(&src.text, lr.start);
        if src.text[para.clone()].trim_start().starts_with('\\')
            && src.text[para.clone()].lines().count() == 1
        {
            continue; // headings and lone commands are not paragraphs
        }
        if !seen.iter().any(|(sf, sr)| sf == &f && sr == &para) {
            seen.push((f, para));
        }
    }
    seen.into_iter().nth(k.saturating_sub(1))
}

/// Byte range of the blank-line-separated paragraph containing `offset`.
fn paragraph_around(text: &str, offset: usize) -> std::ops::Range<usize> {
    let start = text[..offset].rfind("\n\n").map_or(0, |i| i + 2);
    let end = text[offset..]
        .find("\n\n")
        .map_or(text.len(), |i| offset + i);
    start..end
}

/// The `k`-th paragraph (1-based) inside `range`, skipping the heading line.
fn nth_paragraph(
    text: &str,
    range: std::ops::Range<usize>,
    k: usize,
) -> Option<std::ops::Range<usize>> {
    let body = &text[range.clone()];
    let mut offset = range.start;
    let mut count = 0;
    for chunk in body.split("\n\n") {
        let trimmed = chunk.trim();
        let start = offset + chunk.len() - chunk.trim_start().len();
        offset += chunk.len() + 2;
        if trimmed.is_empty()
            || (trimmed.starts_with('\\') && !trimmed.contains('\n') && trimmed.ends_with('}'))
        {
            continue;
        }
        count += 1;
        if count == k {
            return Some(start..start + trimmed.len());
        }
    }
    None
}

#[cfg(test)]
mod tests;
