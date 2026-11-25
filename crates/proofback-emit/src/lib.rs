//! Output formats: Markdown checklist, response letter, SARIF and the answered PDF.

use std::fmt::Write as _;

use proofback_model::{Item, Origin, Status};
use proofback_pdf::{AnnotationSpec, Limits, read_annotations, strip_metadata, write_annotations};

/// GitHub task list, one line per item.
pub fn markdown_checklist(items: &[&Item]) -> String {
    let mut out = String::new();
    for it in items {
        let done = if matches!(
            it.status,
            Status::Done | Status::Wontfix | Status::Duplicate
        ) {
            "x"
        } else {
            " "
        };
        let loc = it.target.as_ref().map_or_else(
            || page_of(it).map_or_else(|| "unlocated".into(), |p| format!("p. {p}")),
            |t| format!("{}:{}", t.file, t.line),
        );
        let body = if it.body.is_empty() {
            "(no note)".to_owned()
        } else {
            one_line(&it.body)
        };
        let _ = writeln!(
            out,
            "- [{done}] `{}` **{}** `{loc}`: {} _({})_",
            it.id,
            it.kind.as_str(),
            md_escape(&body),
            it.status.as_str()
        );
    }
    out
}

fn page_of(it: &Item) -> Option<u32> {
    match &it.origin {
        Origin::Pdf { page, .. } => Some(*page),
        Origin::Notes { page, .. } => *page,
        Origin::Docx { .. } => None,
    }
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn md_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '|') {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

/// Letter formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LetterFormat {
    Markdown,
    Latex,
}

/// A response letter: every item quoted, with status, replies and location.
pub fn letter(items: &[&Item], title: &str, format: LetterFormat) -> String {
    match format {
        LetterFormat::Markdown => letter_md(items, title),
        LetterFormat::Latex => letter_tex(items, title),
    }
}

fn status_sentence(s: Status) -> &'static str {
    match s {
        Status::Done => "Addressed.",
        Status::Wontfix => "Not changed.",
        Status::Discuss => "Open for discussion.",
        Status::Duplicate => "Duplicate of another comment.",
        Status::Open => "Not yet addressed.",
    }
}

fn letter_md(items: &[&Item], title: &str) -> String {
    let mut out = format!("# {}\n\n", md_escape(title));
    let mut n = 0;
    for (round, group) in group_by_round(items) {
        let _ = writeln!(out, "## {}\n", md_escape(&round));
        for it in group {
            n += 1;
            let who = it.author.as_deref().unwrap_or("Reviewer");
            let _ = writeln!(out, "### {n}. {} ({})\n", md_escape(who), location(it));
            let quote = if it.body.is_empty() {
                format!("[{}]", it.kind.as_str())
            } else {
                it.body.clone()
            };
            for l in quote.lines() {
                let _ = writeln!(out, "> {}", md_escape(l));
            }
            let _ = writeln!(out, "\n**{}**", status_sentence(it.status));
            for r in &it.replies {
                let _ = writeln!(out, "\n{}", md_escape(&r.body));
            }
            if let Some(c) = &it.resolution.commit {
                let _ = writeln!(out, "\n_Change: commit `{c}`._");
            }
            out.push('\n');
        }
    }
    out
}

fn letter_tex(items: &[&Item], title: &str) -> String {
    let mut out = String::from(
        "\\documentclass[11pt]{article}\n\\usepackage[T1]{fontenc}\n\\usepackage{xcolor}\n\\usepackage[margin=2.5cm]{geometry}\n\\newenvironment{reviewer}{\\par\\noindent\\color{gray!60!black}\\itshape}{\\par}\n\\begin{document}\n",
    );
    let _ = writeln!(out, "\\section*{{{}}}", tex_escape(title));
    let mut n = 0;
    for (round, group) in group_by_round(items) {
        let _ = writeln!(out, "\\subsection*{{{}}}", tex_escape(&round));
        for it in group {
            n += 1;
            let who = it.author.as_deref().unwrap_or("Reviewer");
            let _ = writeln!(
                out,
                "\\paragraph{{{n}. {} ({})}}",
                tex_escape(who),
                tex_escape(&location(it))
            );
            let quote = if it.body.is_empty() {
                format!("[{}]", it.kind.as_str())
            } else {
                it.body.clone()
            };
            let _ = writeln!(
                out,
                "\\begin{{reviewer}}{}\\end{{reviewer}}",
                tex_escape(&quote)
            );
            let _ = writeln!(out, "\\noindent\\textbf{{{}}}", status_sentence(it.status));
            for r in &it.replies {
                let _ = writeln!(out, "{}\n", tex_escape(&r.body));
            }
            if let Some(c) = &it.resolution.commit {
                let _ = writeln!(
                    out,
                    "\\emph{{Change: commit \\texttt{{{}}}.}}\n",
                    tex_escape(c)
                );
            }
        }
    }
    out.push_str("\\end{document}\n");
    out
}

fn location(it: &Item) -> String {
    let page = page_of(it).map(|p| format!("p. {p}"));
    let src = it.target.as_ref().map(|t| format!("{}:{}", t.file, t.line));
    match (page, src) {
        (Some(p), Some(s)) => format!("{p}, {s}"),
        (Some(p), None) => p,
        (None, Some(s)) => s,
        (None, None) => "general".into(),
    }
}

fn group_by_round<'a>(items: &[&'a Item]) -> Vec<(String, Vec<&'a Item>)> {
    let mut groups: Vec<(String, Vec<&Item>)> = Vec::new();
    for it in items {
        match groups.iter_mut().find(|(r, _)| r == &it.round) {
            Some((_, v)) => v.push(it),
            None => groups.push((it.round.clone(), vec![it])),
        }
    }
    groups
}

/// Escape text for LaTeX, including characters that would run commands.
pub fn tex_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\textbackslash{}"),
            '{' => o.push_str("\\{"),
            '}' => o.push_str("\\}"),
            '$' => o.push_str("\\$"),
            '&' => o.push_str("\\&"),
            '#' => o.push_str("\\#"),
            '%' => o.push_str("\\%"),
            '_' => o.push_str("\\_"),
            '^' => o.push_str("\\textasciicircum{}"),
            '~' => o.push_str("\\textasciitilde{}"),
            '<' => o.push_str("\\textless{}"),
            '>' => o.push_str("\\textgreater{}"),
            '\n' => o.push_str("\\\\\n"),
            c => o.push(c),
        }
    }
    o
}

/// SARIF 2.1.0 log with one result per open item that has a source location.
pub fn sarif(items: &[&Item], version: &str) -> serde_json::Value {
    let results: Vec<serde_json::Value> = items
        .iter()
        .filter(|it| matches!(it.status, Status::Open | Status::Discuss))
        .filter_map(|it| {
            let t = it.target.as_ref()?;
            let text = if it.body.is_empty() { format!("Reviewer marked this text ({})", it.kind.as_str()) } else { it.body.clone() };
            Some(serde_json::json!({
                "ruleId": format!("proofback/{}", it.kind.as_str()),
                "level": if it.is_edit() { "warning" } else { "note" },
                "message": { "text": format!("{} [{}]", text, it.id) },
                "partialFingerprints": { "proofbackItem": it.id.to_string() },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": t.file },
                        "region": {
                            "startLine": t.line,
                            "startColumn": t.column,
                            "endLine": t.end_line,
                            "endColumn": t.end_column.max(t.column + u32::from(t.end_line == t.line && t.end_column == t.column)),
                        }
                    }
                }]
            }))
        })
        .collect();
    let kinds = [
        "comment",
        "question",
        "delete",
        "insert",
        "replace",
        "highlight",
        "figure",
        "math",
        "general",
    ];
    serde_json::json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": { "driver": {
                "name": "proofback",
                "version": version,
                "informationUri": "https://amet13.github.io/proofback/",
                "rules": kinds.iter().map(|k| serde_json::json!({
                    "id": format!("proofback/{k}"),
                    "shortDescription": { "text": format!("Reviewer feedback: {k}") }
                })).collect::<Vec<_>>()
            }},
            "results": results
        }]
    })
}

/// The reviewer's PDF with a reply note on every answered item. The original is not
/// modified; document metadata is stripped from the copy.
pub fn answer_pdf(
    original: &[u8],
    items: &[&Item],
    author: &str,
) -> Result<(Vec<u8>, usize), proofback_pdf::Error> {
    let (_, annots) = read_annotations(original, Limits::default())?;
    let mut specs = Vec::new();
    for it in items {
        let Origin::Pdf {
            page,
            nm,
            rect,
            subtype,
            ..
        } = &it.origin
        else {
            continue;
        };
        let found = annots.iter().find(|a| {
            a.page == *page
                && match nm {
                    Some(nm) => a.nm.as_deref() == Some(nm.as_str()),
                    None => {
                        &a.subtype == subtype
                            && a.rect.iter().zip(rect).all(|(x, y)| (x - y).abs() < 0.6)
                    }
                }
        });
        let Some(a) = found else { continue };
        if it.status == Status::Open && it.replies.is_empty() {
            continue;
        }
        let mut text = format!("{} ({})", status_sentence(it.status), it.status.as_str());
        for r in &it.replies {
            text.push_str("\n\n");
            text.push_str(&r.body);
        }
        if let Some(c) = &it.resolution.commit {
            let _ = write!(text, "\n\nCommit {c}");
        }
        let x = a.rect[2] + 2.0;
        let mut spec =
            AnnotationSpec::new(a.page, "Text", [x, a.rect[3] - 18.0, x + 18.0, a.rect[3]]);
        spec.contents = Some(text);
        spec.author = Some(author.to_owned());
        spec.in_reply_to = Some((a.page, a.index));
        spec.nm = Some(format!("proofback-reply-{}", it.id));
        spec.names.push(("Name".into(), "Comment".into()));
        specs.push(spec);
    }
    let n = specs.len();
    let with = write_annotations(original, &specs)?;
    Ok((strip_metadata(&with)?, n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proofback_model::{
        Anchor, AnchorState, ByteRange, Confidence, ItemId, Kind, Quote, Reply, Target,
    };

    fn item() -> Item {
        Item {
            id: ItemId::derive("r", "x"),
            round: "2026-11-25-supervisor".into(),
            kind: Kind::Question,
            author: Some("Prof. Hale".into()),
            created: None,
            body: "Why 50% & not $x$?".into(),
            suggestion: None,
            target: Some(Target {
                file: "ch/a.tex".into(),
                range: ByteRange { start: 0, end: 4 },
                line: 3,
                column: 1,
                end_line: 3,
                end_column: 5,
                quote: Quote::default(),
            }),
            anchor: Anchor {
                state: AnchorState::Anchored,
                confidence: Confidence::Exact,
                reason: String::new(),
                pending: None,
            },
            origin: Origin::Pdf {
                page: 2,
                subtype: "Text".into(),
                nm: None,
                quads: vec![],
                rect: [0.0; 4],
                marked_text: String::new(),
                color: None,
            },
            status: Status::Done,
            replies: vec![Reply {
                author: "me".into(),
                body: "Chosen by CV_2.".into(),
                at: "t".into(),
            }],
            resolution: proofback_model::Resolution::default(),
            thread: vec![],
        }
    }

    #[test]
    fn checklist_and_letters_escape_untrusted_text() {
        let it = item();
        let md = markdown_checklist(&[&it]);
        assert!(md.starts_with("- [x] `pb_"), "{md}");
        let tex = letter(&[&it], "Response", LetterFormat::Latex);
        assert!(tex.contains("Why 50\\% \\& not \\$x\\$?"), "{tex}");
        assert!(tex.contains("CV\\_2"));
        assert!(tex.contains("p. 2, ch/a.tex:3"));
        let md = letter(&[&it], "Response", LetterFormat::Markdown);
        assert!(md.contains("> Why 50% & not $x$?"));
        assert_eq!(
            tex_escape("\\input{/etc/passwd}"),
            "\\textbackslash{}input\\{/etc/passwd\\}"
        );
    }

    #[test]
    fn sarif_has_locations_for_open_items() {
        let mut it = item();
        it.status = Status::Open;
        let s = sarif(&[&it], "0.1.0");
        assert_eq!(s["version"], "2.1.0");
        assert_eq!(
            s["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"]["startLine"],
            3
        );
        it.status = Status::Done;
        assert_eq!(
            sarif(&[&it], "0.1.0")["runs"][0]["results"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
}
