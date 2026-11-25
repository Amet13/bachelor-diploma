//! Reading tracked changes and comments from a DOCX and anchoring them.

use std::collections::HashMap;
use std::io::Read as _;
use std::ops::Range;

use proofback_anchor::{Sources, align, make_target};
use proofback_model::{
    Anchor, AnchorState, Confidence, Item, ItemId, Kind, Origin, Reply, Status, Suggestion, Target,
};
use proofback_tex::PLACEHOLDER;
use quick_xml::Reader;
use quick_xml::events::Event;

use crate::export::unescape;
use crate::{Error, MARK_CLOSE, MARK_OPEN, MAX_ENTRIES, MAX_PART_BYTES};

/// What a change does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Insert,
    Delete,
    Replace,
    Comment,
}

/// One tracked change or comment, positioned in the paragraph's original text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub kind: ChangeKind,
    /// Word's id of the change or comment.
    pub word_id: String,
    pub author: Option<String>,
    pub date: Option<String>,
    /// Comment text, or the inserted/replacement text.
    pub text: String,
    /// Character range in `paragraph_text` (original text, before the changes).
    pub range: Range<usize>,
    /// The paragraph's original text (normal plus deleted runs).
    pub paragraph_text: String,
    /// Source bookmark of the paragraph, if any.
    pub bookmark: Option<String>,
    /// Replies (for comments).
    pub replies: Vec<(String, String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seg {
    Normal,
    Ins,
    Del,
}

#[derive(Debug, Default)]
struct Para {
    bookmark: Option<String>,
    /// (kind, text, author, date, word id)
    segs: Vec<(Seg, String, Option<String>, Option<String>, String)>,
    /// comment id -> (start char, end char) in original text
    comments: HashMap<String, (usize, Option<usize>)>,
    orig_len: usize,
}

fn attr(e: &quick_xml::events::BytesStart<'_>, name: &str) -> Option<String> {
    e.try_get_attribute(name)
        .ok()
        .flatten()
        .map(|a| unescape(&a.value))
}

fn read_parts(docx: &[u8]) -> Result<HashMap<String, String>, Error> {
    let mut ar = zip::ZipArchive::new(std::io::Cursor::new(docx))
        .map_err(|e| Error::Archive(e.to_string()))?;
    if ar.len() > MAX_ENTRIES {
        return Err(Error::Limit {
            what: "entry count",
        });
    }
    let wanted = |n: &str| {
        n == "word/document.xml"
            || n == "word/comments.xml"
            || n == "word/commentsExtended.xml"
            || (n.starts_with("customXml/item")
                && std::path::Path::new(n)
                    .extension()
                    .is_some_and(|e| e == "xml"))
    };
    let mut out = HashMap::new();
    let mut total = 0u64;
    for i in 0..ar.len() {
        let mut f = ar.by_index(i).map_err(|e| Error::Archive(e.to_string()))?;
        let name = f
            .name()
            .map_err(|e| Error::Archive(e.to_string()))?
            .into_owned();
        if !wanted(&name) {
            continue;
        }
        let mut data = Vec::new();
        (&mut f)
            .take(MAX_PART_BYTES + 1)
            .read_to_end(&mut data)
            .map_err(|e| Error::Archive(e.to_string()))?;
        if data.len() as u64 > MAX_PART_BYTES {
            return Err(Error::Limit { what: "part size" });
        }
        total += data.len() as u64;
        if total > MAX_PART_BYTES * 2 {
            return Err(Error::Limit { what: "total size" });
        }
        out.insert(name, String::from_utf8_lossy(&data).into_owned());
    }
    if !out.contains_key("word/document.xml") {
        return Err(Error::Archive("word/document.xml is missing".into()));
    }
    Ok(out)
}

/// Comment id -> (author, date, text, paraId); and paraId -> parent paraId.
#[allow(clippy::type_complexity)]
fn read_comments(
    xml: Option<&String>,
    ext: Option<&String>,
) -> Result<
    (
        HashMap<String, (Option<String>, Option<String>, String, Option<String>)>,
        HashMap<String, String>,
    ),
    Error,
> {
    let mut comments = HashMap::new();
    if let Some(xml) = xml {
        let mut r = Reader::from_str(xml);
        let mut cur: Option<(
            String,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
        )> = None;
        let mut in_t = false;
        loop {
            match r.read_event().map_err(|e| Error::Xml(e.to_string()))? {
                Event::Start(e) => match e.local_name().as_ref() {
                    "comment" => {
                        cur = Some((
                            attr(&e, "w:id").unwrap_or_default(),
                            attr(&e, "w:author"),
                            attr(&e, "w:date"),
                            String::new(),
                            None,
                        ));
                    }
                    "p" => {
                        if let Some(c) = cur.as_mut() {
                            if c.4.is_none() {
                                c.4 = attr(&e, "w14:paraId");
                            }
                            if !c.3.is_empty() {
                                c.3.push('\n');
                            }
                        }
                    }
                    "t" => in_t = true,
                    _ => {}
                },
                Event::Text(t) => {
                    if in_t && let Some(c) = cur.as_mut() {
                        c.3.push_str(&t.xml10_content());
                    }
                }
                Event::GeneralRef(g) => {
                    if in_t && let Some(c) = cur.as_mut() {
                        c.3.push_str(&general_ref(&g));
                    }
                }
                Event::End(e) => match e.local_name().as_ref() {
                    "t" => in_t = false,
                    "comment" => {
                        if let Some((id, a, d, text, pid)) = cur.take() {
                            comments.insert(id, (a, d, text.trim().to_owned(), pid));
                        }
                    }
                    _ => {}
                },
                Event::Eof => break,
                _ => {}
            }
        }
    }
    let mut parents = HashMap::new();
    if let Some(xml) = ext {
        let mut r = Reader::from_str(xml);
        loop {
            match r.read_event().map_err(|e| Error::Xml(e.to_string()))? {
                Event::Empty(e) | Event::Start(e) if e.local_name().as_ref() == "commentEx" => {
                    if let (Some(p), Some(parent)) =
                        (attr(&e, "w15:paraId"), attr(&e, "w15:paraIdParent"))
                    {
                        parents.insert(p, parent);
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
    }
    Ok((comments, parents))
}

fn general_ref(g: &quick_xml::events::BytesRef<'_>) -> String {
    if let Ok(Some(c)) = g.resolve_char_ref() {
        return c.to_string();
    }
    match g.xml10_content().as_ref() {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" => "'".into(),
        _ => String::new(),
    }
}

/// Read all tracked changes and comments.
pub fn read_changes(docx: &[u8]) -> Result<(Vec<Change>, Vec<String>), Error> {
    let parts = read_parts(docx)?;
    let files = parts
        .iter()
        .filter(|(k, _)| k.starts_with("customXml/"))
        .find_map(|(_, v)| {
            v.contains("amet13.github.io/proofback/docx")
                .then(|| anchor_files(v))
        })
        .unwrap_or_default();
    let (comments, parents) = read_comments(
        parts.get("word/comments.xml"),
        parts.get("word/commentsExtended.xml"),
    )?;
    let xml = &parts["word/document.xml"];
    let mut r = Reader::from_str(xml);
    let mut paras: Vec<Para> = Vec::new();
    let mut cur: Option<Para> = None;
    let mut seg = Seg::Normal;
    let mut seg_meta: (Option<String>, Option<String>, String) = (None, None, String::new());
    let mut in_text = false;
    let mut depth_p = 0usize;
    loop {
        let ev = r.read_event().map_err(|e| Error::Xml(e.to_string()))?;
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev, Event::Empty(_));
                match e.local_name().as_ref() {
                    "p" if !empty => {
                        depth_p += 1;
                        if depth_p == 1 {
                            cur = Some(Para::default());
                        }
                    }
                    "ins" if !empty => {
                        seg = Seg::Ins;
                        seg_meta = (
                            attr(e, "w:author"),
                            attr(e, "w:date"),
                            attr(e, "w:id").unwrap_or_default(),
                        );
                    }
                    "del" if !empty => {
                        seg = Seg::Del;
                        seg_meta = (
                            attr(e, "w:author"),
                            attr(e, "w:date"),
                            attr(e, "w:id").unwrap_or_default(),
                        );
                    }
                    "t" | "delText" if !empty => in_text = true,
                    "tab" => push_text(&mut cur, seg, &seg_meta, "\t"),
                    "br" | "cr" => push_text(&mut cur, seg, &seg_meta, " "),
                    "bookmarkStart" => {
                        if let (Some(p), Some(n)) = (cur.as_mut(), attr(e, "w:name"))
                            && n.starts_with("pb_")
                            && p.bookmark.is_none()
                        {
                            p.bookmark = Some(n);
                        }
                    }
                    "commentRangeStart" => {
                        if let (Some(p), Some(id)) = (cur.as_mut(), attr(e, "w:id")) {
                            let at = p.orig_len;
                            p.comments.insert(id, (at, None));
                        }
                    }
                    "commentRangeEnd" => {
                        if let (Some(p), Some(id)) = (cur.as_mut(), attr(e, "w:id")) {
                            let at = p.orig_len;
                            p.comments.entry(id).or_insert((at, None)).1 = Some(at);
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                if in_text {
                    push_text(&mut cur, seg, &seg_meta, &t.xml10_content());
                }
            }
            Event::GeneralRef(g) => {
                if in_text {
                    push_text(&mut cur, seg, &seg_meta, &general_ref(&g));
                }
            }
            Event::End(e) => match e.local_name().as_ref() {
                "t" | "delText" => in_text = false,
                "ins" | "del" => seg = Seg::Normal,
                "p" => {
                    depth_p = depth_p.saturating_sub(1);
                    if depth_p == 0
                        && let Some(p) = cur.take()
                    {
                        paras.push(p);
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        if paras.len() > 200_000 {
            return Err(Error::Limit {
                what: "paragraph count",
            });
        }
    }
    let mut out = Vec::new();
    for p in &paras {
        let original: String = p
            .segs
            .iter()
            .filter(|s| s.0 != Seg::Ins)
            .map(|s| s.1.as_str())
            .collect();
        // Walk segments, tracking positions in the original text.
        let mut pos = 0usize;
        let mut i = 0;
        while i < p.segs.len() {
            let (kind, text, author, date, id) = &p.segs[i];
            let len = text.chars().count();
            match kind {
                Seg::Normal => pos += len,
                Seg::Del => {
                    // Deletion followed by an insertion at the same place: a replacement.
                    let next_ins = p.segs.get(i + 1).filter(|s| s.0 == Seg::Ins);
                    let (k, t) = match next_ins {
                        Some(n) => (ChangeKind::Replace, n.1.clone()),
                        None => (ChangeKind::Delete, String::new()),
                    };
                    out.push(Change {
                        kind: k,
                        word_id: id.clone(),
                        author: author.clone(),
                        date: date.clone(),
                        text: t,
                        range: pos..pos + len,
                        paragraph_text: original.clone(),
                        bookmark: p.bookmark.clone(),
                        replies: vec![],
                    });
                    pos += len;
                    if next_ins.is_some() {
                        i += 1;
                    }
                }
                Seg::Ins => {
                    out.push(Change {
                        kind: ChangeKind::Insert,
                        word_id: id.clone(),
                        author: author.clone(),
                        date: date.clone(),
                        text: text.clone(),
                        range: pos..pos,
                        paragraph_text: original.clone(),
                        bookmark: p.bookmark.clone(),
                        replies: vec![],
                    });
                }
            }
            i += 1;
        }
        let mut ids: Vec<&String> = p.comments.keys().collect();
        ids.sort();
        for id in ids {
            let (s, e) = p.comments[id];
            let Some((author, date, text, para_id)) = comments.get(id) else {
                continue;
            };
            // Replies are comments whose paragraph has a parent; they are attached below.
            if para_id
                .as_ref()
                .is_some_and(|pid| parents.contains_key(pid))
            {
                continue;
            }
            let replies = comments
                .values()
                .filter(|(_, _, _, pid)| {
                    pid.as_ref().and_then(|pid| parents.get(pid)) == para_id.as_ref()
                        && para_id.is_some()
                })
                .map(|(a, d, t, _)| {
                    (
                        a.clone().unwrap_or_default(),
                        t.clone(),
                        d.clone().unwrap_or_default(),
                    )
                })
                .collect();
            out.push(Change {
                kind: ChangeKind::Comment,
                word_id: id.clone(),
                author: author.clone(),
                date: date.clone(),
                text: text.clone(),
                range: s..e.unwrap_or(s).max(s),
                paragraph_text: original.clone(),
                bookmark: p.bookmark.clone(),
                replies,
            });
        }
    }
    Ok((out, files))
}

fn push_text(
    cur: &mut Option<Para>,
    seg: Seg,
    meta: &(Option<String>, Option<String>, String),
    text: &str,
) {
    let Some(p) = cur.as_mut() else { return };
    if seg != Seg::Ins {
        p.orig_len += text.chars().count();
    }
    match p.segs.last_mut() {
        Some(last) if last.0 == seg && last.4 == meta.2 => last.1.push_str(text),
        _ => p.segs.push((
            seg,
            text.to_owned(),
            meta.0.clone(),
            meta.1.clone(),
            meta.2.clone(),
        )),
    }
}

fn anchor_files(xml: &str) -> Vec<String> {
    let mut files = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<file index=\"") {
        let after = &rest[i..];
        let Some(gt) = after.find('>') else { break };
        let Some(end) = after.find("</file>") else {
            break;
        };
        files.push(unescape(&after[gt + 1..end]));
        rest = &after[end..];
    }
    files
}

/// Folded characters of DOCX text with `⟨…⟩` stand-ins turned into placeholders,
/// plus a map from folded index to original character index.
fn fold_docx(s: &str) -> (Vec<char>, Vec<usize>) {
    let mut chars = Vec::new();
    let mut origin = Vec::new();
    let mut tmp = Vec::new();
    let mut in_mark = false;
    for (i, c) in s.chars().enumerate() {
        if c == MARK_OPEN {
            in_mark = true;
            chars.push(PLACEHOLDER);
            origin.push(i);
            continue;
        }
        if in_mark {
            if c == MARK_CLOSE {
                in_mark = false;
            }
            continue;
        }
        tmp.clear();
        proofback_tex::fold_char(c, &mut tmp);
        for &f in &tmp {
            if f == ' ' && matches!(chars.last(), None | Some(' ')) {
                continue;
            }
            chars.push(f);
            origin.push(i);
        }
    }
    (chars, origin)
}

/// Anchor Word changes and build items.
pub fn import(docx: &[u8], sources: &Sources, round: &str) -> Result<Vec<Item>, Error> {
    let (changes, files) = read_changes(docx)?;
    let mut items = Vec::new();
    let mut taken = std::collections::HashSet::new();
    for c in &changes {
        let (target, confidence, reason) = locate(c, &files, sources);
        let (kind, body, suggestion) = match c.kind {
            ChangeKind::Comment => (
                if c.text.contains('?') {
                    Kind::Question
                } else {
                    Kind::Comment
                },
                c.text.clone(),
                None,
            ),
            ChangeKind::Delete => (Kind::Delete, "Tracked deletion".to_owned(), None),
            ChangeKind::Insert => (
                Kind::Insert,
                format!("Tracked insertion: “{}”", c.text),
                Some(Suggestion {
                    replace_with: None,
                    insert: Some(c.text.clone()),
                }),
            ),
            ChangeKind::Replace => (
                Kind::Replace,
                format!("Tracked change: replace with “{}”", c.text),
                Some(Suggestion {
                    replace_with: Some(c.text.clone()),
                    insert: None,
                }),
            ),
        };
        let change = match c.kind {
            ChangeKind::Comment => "comment",
            ChangeKind::Insert => "insertion",
            ChangeKind::Delete => "deletion",
            ChangeKind::Replace => "replacement",
        };
        let id = ItemId::derive_unique(
            round,
            &format!("docx:{change}:{}:{}", c.word_id, c.range.start),
            |i| taken.contains(i),
        );
        taken.insert(id.clone());
        items.push(Item {
            id,
            round: round.to_owned(),
            kind,
            author: c.author.clone(),
            created: c.date.clone(),
            body,
            suggestion,
            target,
            anchor: Anchor {
                state: AnchorState::Anchored,
                confidence,
                reason,
                pending: None,
            },
            origin: Origin::Docx {
                change: change.into(),
                word_id: c.word_id.clone(),
                bookmark: c.bookmark.clone(),
            },
            status: Status::Open,
            replies: Vec::new(),
            resolution: proofback_model::Resolution::default(),
            thread: c
                .replies
                .iter()
                .map(|(a, t, d)| Reply {
                    author: a.clone(),
                    body: t.clone(),
                    at: d.clone(),
                })
                .collect(),
        });
    }
    Ok(items)
}

type Located = (Option<Target>, Confidence, String);

fn locate(c: &Change, files: &[String], sources: &Sources) -> Located {
    // Window: the bookmarked paragraph, or the whole project.
    let bookmark = c.bookmark.as_deref().and_then(|b| parse_bookmark(b, files));
    let (para_fold, para_origin) = fold_docx(&c.paragraph_text);
    let total_chars = c.paragraph_text.chars().count();
    // Folded range of the change inside the paragraph.
    let fstart = para_origin
        .iter()
        .position(|&o| o >= c.range.start)
        .unwrap_or(para_fold.len());
    let fend = para_origin
        .iter()
        .position(|&o| o >= c.range.end)
        .unwrap_or(para_fold.len());
    let ctx = 30;
    let q_start = fstart.saturating_sub(ctx);
    let q_end = (fend + ctx).min(para_fold.len());
    let query: Vec<char> = para_fold[q_start..q_end].to_vec();
    let _ = total_chars;
    if query.iter().filter(|c| c.is_alphanumeric()).count() < 4 {
        return (
            None,
            Confidence::Page,
            "too little text around the change".into(),
        );
    }
    let candidates: Vec<(String, Range<usize>)> = match &bookmark {
        Some((file, range)) => sources
            .get(file)
            .and_then(|s| s.folded_range_for_source(range.clone()))
            .map(|r| vec![(file.clone(), r)])
            .unwrap_or_default(),
        None => sources
            .files()
            .map(|(f, s)| (f.to_owned(), 0..s.folded.chars.len()))
            .collect(),
    };
    let max_cost = if query.len() < 12 {
        0
    } else {
        query.len() / 12
    };
    let mut hits = Vec::new();
    for (file, win) in &candidates {
        let src = sources.get(file).expect("candidate");
        let al = align::align(&query, &src.folded.chars[win.clone()], max_cost);
        for h in al.best {
            hits.push((file.clone(), win.start + h.start, win.start + h.end, h.cost));
        }
    }
    let min = hits.iter().map(|h| h.3).min();
    hits.retain(|h| Some(h.3) == min);
    let [(file, hs, he, cost)] = hits.as_slice() else {
        let why = if hits.is_empty() {
            "text around the change not found in the source"
        } else {
            "text around the change matches several places"
        };
        return (None, Confidence::Page, why.into());
    };
    let src = sources.get(file).expect("hit");
    // Position of the change inside the hit (exact when the alignment is exact).
    let (a, b) = if *cost == 0 && he - hs == query.len() {
        (hs + (fstart - q_start), hs + (fend - q_start))
    } else {
        let target: Vec<char> = para_fold[fstart..fend].to_vec();
        if target.is_empty() {
            return (
                Some(make_target(file, src, line_of(src, *hs))),
                Confidence::Line,
                "change found near this line".into(),
            );
        }
        let inner = align::align(&target, &src.folded.chars[*hs..*he], target.len() / 10);
        match inner.best.as_slice() {
            [h] => (hs + h.start, hs + h.end),
            _ => {
                return (
                    Some(make_target(file, src, line_of(src, *hs))),
                    Confidence::Line,
                    "change found near this line".into(),
                );
            }
        }
    };
    let range = if a == b {
        let byte = if a == 0 {
            0
        } else {
            src.source_range_for_folded(a - 1..a).map_or(0, |r| r.end)
        };
        byte..byte
    } else {
        match src.source_range_for_folded(a..b) {
            Some(r) => r,
            None => {
                return (
                    None,
                    Confidence::Page,
                    "could not map the change to source".into(),
                );
            }
        }
    };
    let via = if bookmark.is_some() {
        "through its paragraph anchor"
    } else {
        "without anchors (text only)"
    };
    let conf = if *cost == 0 {
        Confidence::Exact
    } else {
        Confidence::Line
    };
    (
        Some(make_target(file, src, range)),
        conf,
        format!("unique match of the surrounding text {via}"),
    )
}

fn line_of(src: &proofback_anchor::SourceFile, folded: usize) -> Range<usize> {
    let byte = src
        .source_range_for_folded(folded..folded + 1)
        .map_or(0, |r| r.start);
    let (line, _) = src.line_col(byte);
    src.line_range(line as usize).unwrap_or(byte..byte)
}

/// `pb_<file index>_<start>_<end>` -> (file, byte range).
fn parse_bookmark(name: &str, files: &[String]) -> Option<(String, Range<usize>)> {
    let mut it = name.strip_prefix("pb_")?.split('_');
    let fi: usize = it.next()?.parse().ok()?;
    let s: usize = it.next()?.parse().ok()?;
    let e: usize = it.next()?.parse().ok()?;
    if s > e {
        return None;
    }
    Some((files.get(fi)?.clone(), s..e))
}
