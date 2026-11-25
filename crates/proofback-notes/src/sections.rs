//! Document section numbering computed from the sources.

use proofback_anchor::Sources;

/// A numbered heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// "3", "3.1", "A.2", ...
    pub number: String,
    /// 0 chapter, 1 section, 2 subsection, 3 subsubsection.
    pub level: usize,
    pub title: String,
    pub file: String,
    /// Byte offset of the heading command.
    pub offset: usize,
}

const COMMANDS: [&str; 4] = ["chapter", "section", "subsection", "subsubsection"];

/// Headings in document order, following `\input` and `\include` from `main`.
pub fn headings(sources: &Sources, main: &str) -> Vec<Heading> {
    let mut raw: Vec<(usize, String, String, usize, bool)> = Vec::new(); // level, title, file, offset, appendix-marker
    let mut visited = std::collections::HashSet::new();
    walk(sources, main, &mut raw, &mut visited, 0);
    let has_chapters = raw.iter().any(|r| r.0 == 0 && !r.4);
    let mut counters = [0usize; 4];
    let mut appendix = false;
    let mut out = Vec::new();
    for (level, title, file, offset, is_appendix) in raw {
        if is_appendix {
            appendix = true;
            counters = [0; 4];
            continue;
        }
        let top = usize::from(!has_chapters);
        if level < top {
            continue;
        }
        counters[level] += 1;
        for c in counters.iter_mut().skip(level + 1) {
            *c = 0;
        }
        let parts: Vec<String> = (top..=level)
            .map(|l| {
                if appendix && l == top {
                    char::from(b'A' + (counters[l].saturating_sub(1) % 26) as u8).to_string()
                } else {
                    counters[l].to_string()
                }
            })
            .collect();
        out.push(Heading {
            number: parts.join("."),
            level,
            title,
            file,
            offset,
        });
    }
    out
}

fn walk(
    sources: &Sources,
    file: &str,
    out: &mut Vec<(usize, String, String, usize, bool)>,
    visited: &mut std::collections::HashSet<String>,
    depth: usize,
) {
    if depth > 32 || !visited.insert(file.to_owned()) {
        return;
    }
    let Some(src) = sources.get(file) else { return };
    let text = &src.text;
    let mut i = 0;
    while let Some(p) = text[i..].find('\\') {
        let at = i + p;
        i = at + 1;
        // Skip commented-out commands.
        let line_start = text[..at].rfind('\n').map_or(0, |n| n + 1);
        if text[line_start..at].contains('%') && !text[line_start..at].contains("\\%") {
            continue;
        }
        let rest = &text[at + 1..];
        let name_len = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
        let name = &rest[..name_len];
        let after = &rest[name_len..];
        if name == "appendix" {
            out.push((0, String::new(), file.to_owned(), at, true));
            continue;
        }
        if name == "input" || name == "include" || name == "subfile" {
            if let Some(arg) = braced(after) {
                let mut target = arg.trim().to_owned();
                if !std::path::Path::new(&target)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("tex"))
                {
                    target.push_str(".tex");
                }
                walk(sources, &target, out, visited, depth + 1);
            }
            continue;
        }
        if let Some(level) = COMMANDS.iter().position(|c| *c == name) {
            if after.starts_with('*') {
                continue;
            }
            let after = skip_optional(after);
            if let Some(title) = braced(after) {
                out.push((level, title.trim().to_owned(), file.to_owned(), at, false));
            }
        }
    }
}

fn skip_optional(s: &str) -> &str {
    let t = s.trim_start();
    if let Some(rest) = t.strip_prefix('[')
        && let Some(end) = rest.find(']')
    {
        return &rest[end + 1..];
    }
    t
}

/// Content of a leading `{...}` group (balanced).
fn braced(s: &str) -> Option<&str> {
    let t = s.trim_start();
    let body = t.strip_prefix('{')?;
    let mut depth = 1usize;
    for (i, c) in body.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&body[..i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// File and byte range of a heading's section: up to the next heading of the same
/// or a higher level in the same file, or the end of the file.
pub fn range_of(
    heads: &[Heading],
    i: usize,
    sources: &Sources,
) -> (String, std::ops::Range<usize>) {
    let h = &heads[i];
    let len = sources.get(&h.file).map_or(h.offset, |s| s.text.len());
    let end = heads[i + 1..]
        .iter()
        .find(|n| n.level <= h.level)
        .filter(|n| n.file == h.file)
        .map_or(len, |n| n.offset);
    (h.file.clone(), h.offset..end.max(h.offset))
}
