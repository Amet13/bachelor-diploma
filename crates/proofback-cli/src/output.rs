//! Terminal output helpers.

use std::io::IsTerminal;

use proofback_model::{Confidence, Item, Origin, Status};

/// Whether to use colour on stdout.
pub fn color_enabled(choice: &str) -> bool {
    match choice {
        "always" => true,
        "never" => false,
        _ => std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
    }
}

pub struct Style {
    pub on: bool,
}

impl Style {
    fn wrap(&self, code: &str, s: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_owned()
        }
    }

    pub fn dim(&self, s: &str) -> String {
        self.wrap("2", s)
    }

    pub fn bold(&self, s: &str) -> String {
        self.wrap("1", s)
    }

    pub fn confidence(&self, c: Confidence) -> String {
        let code = match c {
            Confidence::Exact => "32",
            Confidence::Line => "33",
            Confidence::Region => "36",
            Confidence::Page => "31",
        };
        self.wrap(code, c.as_str())
    }

    pub fn status(&self, s: Status) -> String {
        let code = match s {
            Status::Open => "1",
            Status::Done => "32",
            Status::Wontfix | Status::Duplicate => "2",
            Status::Discuss => "35",
        };
        self.wrap(code, s.as_str())
    }
}

/// Location column for tables.
pub fn location(it: &Item) -> String {
    match &it.target {
        Some(t) if t.end_line != t.line => format!(
            "{}:{}:{}-{}:{}",
            t.file, t.line, t.column, t.end_line, t.end_column
        ),
        Some(t) if t.end_column != t.column => format!(
            "{}:{}:{}-{}",
            t.file,
            t.line,
            t.column,
            t.end_column.saturating_sub(1).max(t.column)
        ),
        Some(t) => format!("{}:{}:{}", t.file, t.line, t.column),
        None => match &it.origin {
            Origin::Pdf { page, .. } => format!("p. {page}"),
            Origin::Notes { page: Some(p), .. } => format!("p. {p}"),
            _ => "unlocated".into(),
        },
    }
}

/// One-line note text with control characters removed, truncated for tables.
pub fn note(it: &Item, width: usize) -> String {
    let s = if it.body.trim().is_empty() {
        match &it.origin {
            Origin::Pdf { marked_text, .. } if !marked_text.is_empty() => {
                format!("(on \"{}\")", marked_text.trim())
            }
            _ => "(no note)".to_owned(),
        }
    } else {
        it.body.clone()
    };
    let clean: String = s
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    if clean.chars().count() > width {
        let mut t: String = clean.chars().take(width.saturating_sub(1)).collect();
        t.push('…');
        t
    } else {
        clean
    }
}

/// Render items as an aligned table.
pub fn table(items: &[&Item], style: &Style) -> String {
    let rows: Vec<[String; 6]> = items
        .iter()
        .map(|it| {
            [
                it.id.to_string(),
                location(it),
                it.kind.as_str().to_owned(),
                note(it, 48),
                it.anchor.confidence.as_str().to_owned(),
                it.status.as_str().to_owned(),
            ]
        })
        .collect();
    let headers = ["id", "where", "kind", "note", "anchor", "status"];
    let mut widths = headers.map(str::len);
    for r in &rows {
        for (w, c) in widths.iter_mut().zip(r.iter()) {
            *w = (*w).max(c.chars().count());
        }
    }
    let pad = |s: &str, w: usize| format!("{s}{}", " ".repeat(w.saturating_sub(s.chars().count())));
    let mut out = String::new();
    let head: Vec<String> = headers.iter().zip(widths).map(|(h, w)| pad(h, w)).collect();
    out.push_str(&style.dim(head.join("  ").trim_end()));
    out.push('\n');
    for (r, it) in rows.iter().zip(items) {
        let cells = [
            pad(&r[0], widths[0]),
            pad(&r[1], widths[1]),
            pad(&r[2], widths[2]),
            pad(&r[3], widths[3]),
            format!(
                "{}{}",
                style.confidence(it.anchor.confidence),
                " ".repeat(widths[4].saturating_sub(r[4].len()))
            ),
            style.status(it.status),
        ];
        out.push_str(cells.join("  ").trim_end());
        out.push('\n');
    }
    out
}
