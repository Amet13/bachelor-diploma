//! DOCX export with hidden source anchors.

use std::io::Write as _;
use std::ops::Range;

use proofback_anchor::Sources;
use proofback_tex::PLACEHOLDER;

use crate::{Error, MARK_CLOSE, MARK_OPEN};

/// A source paragraph in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paragraph {
    pub file: String,
    /// Source byte range of the paragraph.
    pub range: Range<usize>,
    /// Text for the reader, with `⟨…⟩` stand-ins for references and maths.
    pub text: String,
    /// Heading level (1 = chapter or top-level section), if a heading.
    pub heading: Option<u8>,
}

/// Paragraphs of the document, following `\input`/`\include` from `main` and
/// skipping the preamble.
pub fn paragraphs(sources: &Sources, main: &str) -> Vec<Paragraph> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    walk(sources, main, &mut out, &mut seen, 0, true);
    out
}

fn walk(
    sources: &Sources,
    file: &str,
    out: &mut Vec<Paragraph>,
    seen: &mut std::collections::HashSet<String>,
    depth: usize,
    is_main: bool,
) {
    if depth > 32 || !seen.insert(file.to_owned()) {
        return;
    }
    let Some(src) = sources.get(file) else { return };
    let text = &src.text;
    let (mut start, end) = if is_main {
        let s = text
            .find("\\begin{document}")
            .map_or(0, |i| i + "\\begin{document}".len());
        let e = text.find("\\end{document}").unwrap_or(text.len());
        (s, e.max(s))
    } else {
        (0, text.len())
    };
    // Split at \input / \include and recurse in place.
    let mut cursor = start;
    while cursor < end {
        let next = find_input(text, cursor, end);
        let seg_end = next.as_ref().map_or(end, |(s, _, _)| *s);
        split_paragraphs(src, file, start..seg_end, out);
        match next {
            Some((_, after, target)) => {
                walk(sources, &target, out, seen, depth + 1, false);
                start = after;
                cursor = after;
            }
            None => break,
        }
    }
}

fn find_input(text: &str, from: usize, to: usize) -> Option<(usize, usize, String)> {
    let mut i = from;
    while let Some(p) = text[i..to].find('\\') {
        let at = i + p;
        i = at + 1;
        let line_start = text[..at].rfind('\n').map_or(0, |n| n + 1);
        if text[line_start..at].contains('%') {
            continue;
        }
        for cmd in ["input", "include", "subfile"] {
            let rest = &text[at + 1..to];
            if rest.starts_with(cmd) && rest[cmd.len()..].trim_start().starts_with('{') {
                let open = at
                    + 1
                    + cmd.len()
                    + (rest[cmd.len()..].len() - rest[cmd.len()..].trim_start().len());
                let close = text[open..to].find('}')? + open;
                let mut target = text[open + 1..close].trim().to_owned();
                if !std::path::Path::new(&target)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("tex"))
                {
                    target.push_str(".tex");
                }
                return Some((at, close + 1, target));
            }
        }
    }
    None
}

fn split_paragraphs(
    src: &proofback_anchor::SourceFile,
    file: &str,
    range: Range<usize>,
    out: &mut Vec<Paragraph>,
) {
    let text = &src.text;
    for r in proofback_tex::paragraph_ranges(text, range.clone()) {
        let (s, e) = (r.start, r.end);
        let trimmed = text[s..e].trim_start();
        let heading = ["\\chapter", "\\section", "\\subsection", "\\subsubsection"]
            .iter()
            .position(|h| {
                trimmed.starts_with(h)
                    && !trimmed[h.len()..].starts_with(|c: char| c.is_ascii_alphabetic())
            })
            .map(|l| (l + 1) as u8);
        // Collect normalised characters whose source lies in this paragraph.
        let Some(chars) = src.normalized.chars_for_source(s..e) else {
            continue;
        };
        let mut para = String::new();
        let mut last_ph: Option<Range<usize>> = None;
        for i in chars {
            let span = src.normalized.span(i);
            if span.start < s || span.end > e {
                continue;
            }
            let c = src.normalized.chars()[i];
            if c == PLACEHOLDER {
                if last_ph.as_ref() == Some(&span) {
                    continue;
                }
                last_ph = Some(span.clone());
                para.push_str(&label(&text[span]));
            } else {
                para.push(c);
            }
        }
        let para = para.trim().to_owned();
        if para.is_empty() {
            continue;
        }
        out.push(Paragraph {
            file: file.to_owned(),
            range: s..e,
            text: para,
            heading,
        });
    }
}

/// `⟨ref: key⟩`, `⟨cite: key⟩`, `⟨math: …⟩` for a placeholder's source text.
fn label(src: &str) -> String {
    let s = src.trim();
    let inner = s
        .find('{')
        .and_then(|o| s.rfind('}').map(|c| &s[o + 1..c.max(o + 1)]))
        .unwrap_or(s);
    let kind = if s.starts_with('$') || s.starts_with("\\(") {
        "math"
    } else if s.contains("cite") {
        "cite"
    } else if s.contains("ref") {
        "ref"
    } else {
        "tex"
    };
    let body = if kind == "math" {
        s.trim_matches(['$', '\\', '(', ')'])
    } else {
        inner
    };
    let body: String = body
        .chars()
        .filter(|c| *c != MARK_OPEN && *c != MARK_CLOSE)
        .take(80)
        .collect();
    format!("{MARK_OPEN}{kind}: {body}{MARK_CLOSE}")
}

fn xml_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => o.push(c),
        }
    }
    o
}

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// Bookmark name for a paragraph.
pub(crate) fn bookmark_name(file_index: usize, range: &Range<usize>) -> String {
    format!("pb_{file_index}_{}_{}", range.start, range.end)
}

/// Runs for a paragraph's text: stand-ins in grey italics.
fn runs(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(o) = rest.find(MARK_OPEN) {
        let (before, tail) = rest.split_at(o);
        if !before.is_empty() {
            out.push_str(&format!(
                "<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r>",
                xml_escape(before)
            ));
        }
        let c = tail
            .find(MARK_CLOSE)
            .map_or(tail.len(), |c| c + MARK_CLOSE.len_utf8());
        out.push_str(&format!(
            "<w:r><w:rPr><w:i/><w:color w:val=\"6B7280\"/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>",
            xml_escape(&tail[..c])
        ));
        rest = &tail[c..];
    }
    if !rest.is_empty() {
        out.push_str(&format!(
            "<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r>",
            xml_escape(rest)
        ));
    }
    out
}

/// Write a DOCX of `paras`. Every paragraph carries its source bookmark.
pub fn export(paras: &[Paragraph], title: &str) -> Result<Vec<u8>, Error> {
    let files = file_list(paras);
    let mut body = String::new();
    if !title.is_empty() {
        body.push_str(&format!(
            "<w:p><w:pPr><w:pStyle w:val=\"Title\"/></w:pPr>{}</w:p>",
            runs(title)
        ));
    }
    for (i, p) in paras.iter().enumerate() {
        let fi = files.iter().position(|f| f == &p.file).unwrap_or(0);
        let style = p
            .heading
            .map(|h| format!("<w:pPr><w:pStyle w:val=\"Heading{}\"/></w:pPr>", h.min(3)))
            .unwrap_or_default();
        body.push_str(&format!(
            "<w:p>{style}<w:bookmarkStart w:id=\"{i}\" w:name=\"{}\"/>{}<w:bookmarkEnd w:id=\"{i}\"/></w:p>",
            bookmark_name(fi, &p.range),
            runs(&p.text)
        ));
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"{W}\"><w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>"
    );
    let mut zipbuf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut zipbuf);
        let o = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let mut put = |name: &str, data: &str| -> Result<(), Error> {
            z.start_file(name, o)
                .map_err(|e| Error::Archive(e.to_string()))?;
            z.write_all(data.as_bytes())
                .map_err(|e| Error::Archive(e.to_string()))
        };
        put("[Content_Types].xml", CONTENT_TYPES)?;
        put("_rels/.rels", ROOT_RELS)?;
        put("word/_rels/document.xml.rels", DOC_RELS)?;
        put("word/document.xml", &document)?;
        put("word/styles.xml", STYLES)?;
        put("customXml/item1.xml", &anchors_xml(&files))?;
        put(
            "customXml/_rels/item1.xml.rels",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"/>",
        )?;
        z.finish().map_err(|e| Error::Archive(e.to_string()))?;
    }
    Ok(zipbuf.into_inner())
}

fn file_list(paras: &[Paragraph]) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for p in paras {
        if !files.contains(&p.file) {
            files.push(p.file.clone());
        }
    }
    files
}

fn anchors_xml(files: &[String]) -> String {
    let list: String = files
        .iter()
        .enumerate()
        .map(|(i, f)| format!("<file index=\"{i}\">{}</file>", xml_escape(f)))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<proofback xmlns=\"https://amet13.github.io/proofback/docx/1\">{list}</proofback>"
    )
}

/// Add source bookmarks to a DOCX made by another tool (pandoc): each document
/// paragraph whose text matches a source paragraph exactly (after folding) gets
/// that paragraph's bookmark.
pub fn add_bookmarks(docx: &[u8], paras: &[Paragraph]) -> Result<(Vec<u8>, usize), Error> {
    use std::io::Read as _;
    let mut ar = zip::ZipArchive::new(std::io::Cursor::new(docx))
        .map_err(|e| Error::Archive(e.to_string()))?;
    let files = file_list(paras);
    let key = |s: &str| -> String {
        let folded: String = proofback_tex::fold_str(s.chars())
            .chars
            .into_iter()
            .filter(|c| c.is_alphanumeric())
            .collect();
        folded.chars().take(120).collect()
    };
    let mut by_key: std::collections::HashMap<String, Option<usize>> =
        std::collections::HashMap::new();
    for (i, p) in paras.iter().enumerate() {
        let plain: String = strip_marks(&p.text);
        let k = key(&plain);
        if k.len() >= 12 {
            by_key.entry(k).and_modify(|v| *v = None).or_insert(Some(i));
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    let mut added = 0;
    {
        let mut z = zip::ZipWriter::new(&mut out);
        for i in 0..ar.len() {
            let mut f = ar.by_index(i).map_err(|e| Error::Archive(e.to_string()))?;
            let name = f
                .name()
                .map_err(|e| Error::Archive(e.to_string()))?
                .into_owned();
            let mut data = Vec::new();
            (&mut f)
                .take(crate::MAX_PART_BYTES)
                .read_to_end(&mut data)
                .map_err(|e| Error::Archive(e.to_string()))?;
            if name == "word/document.xml" {
                let xml = String::from_utf8_lossy(&data).into_owned();
                let (new_xml, n) = bookmark_paragraphs(&xml, |text| {
                    by_key.get(&key(text)).copied().flatten().map(|pi| {
                        bookmark_name(
                            files.iter().position(|f| f == &paras[pi].file).unwrap_or(0),
                            &paras[pi].range,
                        )
                    })
                });
                added = n;
                data = new_xml.into_bytes();
            }
            let o = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            z.start_file(name, o)
                .map_err(|e| Error::Archive(e.to_string()))?;
            z.write_all(&data)
                .map_err(|e| Error::Archive(e.to_string()))?;
        }
        let o = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        z.start_file("customXml/item1.xml", o)
            .map_err(|e| Error::Archive(e.to_string()))?;
        z.write_all(anchors_xml(&files).as_bytes())
            .map_err(|e| Error::Archive(e.to_string()))?;
        z.finish().map_err(|e| Error::Archive(e.to_string()))?;
    }
    Ok((out.into_inner(), added))
}

fn strip_marks(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0u32;
    for c in s.chars() {
        match c {
            MARK_OPEN => depth += 1,
            MARK_CLOSE => depth = depth.saturating_sub(1),
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Insert bookmarks into `<w:p>` elements whose visible text `name_for` recognises.
fn bookmark_paragraphs(xml: &str, name_for: impl Fn(&str) -> Option<String>) -> (String, usize) {
    let mut out = String::with_capacity(xml.len() + 4096);
    let mut rest = xml;
    let mut id = 900_000;
    let mut added = 0;
    while let Some(start) = find_p_start(rest) {
        out.push_str(&rest[..start]);
        let Some(end_rel) = rest[start..].find("</w:p>") else {
            break;
        };
        let end = start + end_rel + "</w:p>".len();
        let p = &rest[start..end];
        let text = visible_text(p);
        match name_for(&text) {
            Some(name) => {
                let open_end = p.find('>').map_or(0, |i| i + 1);
                let insert_at = p
                    .find("</w:pPr>")
                    .map_or(open_end, |i| i + "</w:pPr>".len());
                out.push_str(&p[..insert_at]);
                out.push_str(&format!(
                    "<w:bookmarkStart w:id=\"{id}\" w:name=\"{name}\"/>"
                ));
                out.push_str(&p[insert_at..p.len() - "</w:p>".len()]);
                out.push_str(&format!("<w:bookmarkEnd w:id=\"{id}\"/></w:p>"));
                id += 1;
                added += 1;
            }
            None => out.push_str(p),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    (out, added)
}

fn find_p_start(s: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = s[from..].find("<w:p") {
        let at = from + i;
        let next = s.as_bytes().get(at + 4).copied();
        if matches!(next, Some(b'>' | b' ' | b'/')) {
            return Some(at);
        }
        from = at + 4;
    }
    None
}

fn visible_text(p: &str) -> String {
    let mut out = String::new();
    let mut rest = p;
    while let Some(i) = rest.find("<w:t") {
        let after = &rest[i + 4..];
        if !after.starts_with(['>', ' ']) {
            rest = after;
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        let Some(close) = after[gt..].find("</w:t>") else {
            break;
        };
        out.push_str(&unescape(&after[gt + 1..gt + close]));
        rest = &after[gt + close..];
    }
    out
}

pub(crate) fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

const DOC_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml" Target="../customXml/item1.xml"/></Relationships>"#;

const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Cambria" w:hAnsi="Cambria"/><w:sz w:val="24"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="300" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:rPr><w:b/><w:sz w:val="40"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:keepNext/><w:spacing w:before="360"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:pPr><w:keepNext/><w:spacing w:before="240"/><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:b/><w:sz w:val="28"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Normal"/><w:pPr><w:keepNext/><w:outlineLvl w:val="2"/></w:pPr><w:rPr><w:b/></w:rPr></w:style></w:styles>"#;
