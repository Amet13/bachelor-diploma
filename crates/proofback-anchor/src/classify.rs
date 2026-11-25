//! Decide what a reviewer asked for from the annotation type and note text.

use proofback_model::{Kind, Suggestion};
use proofback_pdf::PdfAnnotation;

/// Kind and suggestion derived from an annotation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classified {
    pub kind: Kind,
    pub suggestion: Option<Suggestion>,
}

/// Classify an annotation.
pub fn classify(a: &PdfAnnotation) -> Classified {
    let body = a.contents.trim();
    let is_question = body.ends_with('?');
    let kind = match a.subtype.as_str() {
        "StrikeOut" => {
            if body.is_empty() {
                Kind::Delete
            } else if is_question {
                Kind::Question
            } else if replacement(body).is_some() {
                Kind::Replace
            } else {
                Kind::Comment
            }
        }
        "Caret" => {
            if body.is_empty() {
                Kind::Comment
            } else {
                Kind::Insert
            }
        }
        "Highlight" | "Underline" | "Squiggly" => {
            if body.is_empty() {
                Kind::Highlight
            } else if is_question {
                Kind::Question
            } else {
                Kind::Comment
            }
        }
        "Text" | "FreeText" => {
            if is_question {
                Kind::Question
            } else {
                Kind::Comment
            }
        }
        "Square" | "Circle" | "Ink" | "Polygon" | "PolyLine" | "Line" => Kind::General,
        _ => Kind::General,
    };
    Classified {
        kind,
        suggestion: suggestion(kind, body),
    }
}

pub(crate) fn suggestion(kind: Kind, body: &str) -> Option<Suggestion> {
    match kind {
        Kind::Replace => replacement(body).map(|r| Suggestion {
            replace_with: Some(r),
            insert: None,
        }),
        Kind::Insert => {
            let text =
                quoted(body).unwrap_or_else(|| strip_verb(body, &["insert", "add"]).to_owned());
            (!text.is_empty()).then_some(Suggestion {
                replace_with: None,
                insert: Some(text),
            })
        }
        _ => None,
    }
}

/// Replacement text for a strike-out note: a quoted string, or a short plain note.
fn replacement(body: &str) -> Option<String> {
    if let Some(q) = quoted(body) {
        return Some(q);
    }
    let plain = strip_verb(body, &["replace with", "change to", "use", "->", "→"]);
    let words = plain.split_whitespace().count();
    (!plain.is_empty() && words <= 6 && !plain.contains(['?', ':'])).then(|| plain.to_owned())
}

/// The last quoted string in the note (“…”, "…", '…', «…»).
fn quoted(body: &str) -> Option<String> {
    let pairs = [('“', '”'), ('"', '"'), ('‘', '’'), ('«', '»'), ('\'', '\'')];
    for (open, close) in pairs {
        let mut last = None;
        let mut rest = body;
        let mut offset = 0;
        while let Some(s) = rest.find(open) {
            let after = &rest[s + open.len_utf8()..];
            let Some(e) = after.find(close) else { break };
            let inner = &after[..e];
            if !inner.trim().is_empty() {
                last = Some(inner.trim().to_owned());
            }
            let consumed = s + open.len_utf8() + e + close.len_utf8();
            offset += consumed;
            rest = &body[offset..];
        }
        if last.is_some() {
            return last;
        }
    }
    None
}

fn strip_verb<'a>(body: &'a str, verbs: &[&str]) -> &'a str {
    let lower = body.to_lowercase();
    for v in verbs {
        if lower.starts_with(v) && body.is_char_boundary(v.len()) {
            return body[v.len()..].trim_start_matches([':', ' ']).trim();
        }
    }
    body.trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annot(subtype: &str, contents: &str) -> PdfAnnotation {
        PdfAnnotation {
            page: 1,
            index: 0,
            subtype: subtype.into(),
            nm: None,
            author: None,
            contents: contents.into(),
            created: None,
            rect: [0.0; 4],
            quads: vec![],
            color: None,
            in_reply_to: None,
            intent: None,
        }
    }

    #[test]
    fn kinds() {
        assert_eq!(classify(&annot("StrikeOut", "")).kind, Kind::Delete);
        let c = classify(&annot("StrikeOut", "Replace with “robust”"));
        assert_eq!(c.kind, Kind::Replace);
        assert_eq!(
            c.suggestion.unwrap().replace_with.as_deref(),
            Some("robust")
        );
        let c = classify(&annot("StrikeOut", "robust"));
        assert_eq!(
            c.suggestion.unwrap().replace_with.as_deref(),
            Some("robust")
        );
        assert_eq!(
            classify(&annot(
                "StrikeOut",
                "This whole paragraph repeats section two, please merge it there"
            ))
            .kind,
            Kind::Comment
        );
        let c = classify(&annot("Caret", "Insert “and Fig. 3.2”"));
        assert_eq!(c.kind, Kind::Insert);
        assert_eq!(
            c.suggestion.unwrap().insert.as_deref(),
            Some("and Fig. 3.2")
        );
        assert_eq!(
            classify(&annot("Caret", "and Fig. 3.2"))
                .suggestion
                .unwrap()
                .insert
                .as_deref(),
            Some("and Fig. 3.2")
        );
        assert_eq!(classify(&annot("Highlight", "")).kind, Kind::Highlight);
        assert_eq!(
            classify(&annot("Highlight", "Cite the field study")).kind,
            Kind::Comment
        );
        assert_eq!(
            classify(&annot("Text", "Why this threshold?")).kind,
            Kind::Question
        );
        assert_eq!(classify(&annot("Ink", "")).kind, Kind::General);
    }
}
