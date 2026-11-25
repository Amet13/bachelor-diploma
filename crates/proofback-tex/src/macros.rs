//! Collect simple user macros from a preamble.

use std::collections::HashMap;

/// Collect parameterless macro definitions (`\newcommand`, `\renewcommand`,
/// `\providecommand`, `\def`) from `src`. Macros that take parameters are skipped:
/// their output cannot be predicted without a TeX engine.
pub fn collect_macros(src: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let b = src.as_bytes();
    let mut i = 0;
    while let Some(p) = src[i..].find('\\') {
        let at = i + p;
        let rest = &src[at + 1..];
        let kw = [
            "newcommand",
            "renewcommand",
            "providecommand",
            "def",
            "gdef",
        ]
        .into_iter()
        .find(|k| {
            rest.starts_with(k)
                && !rest
                    .as_bytes()
                    .get(k.len())
                    .is_some_and(u8::is_ascii_alphabetic)
        });
        let Some(kw) = kw else {
            i = at + 1;
            continue;
        };
        let mut j = at + 1 + kw.len();
        if b.get(j) == Some(&b'*') {
            j += 1;
        }
        j = skip_ws(b, j);
        // Name: `{\name}` or `\name`.
        let braced = b.get(j) == Some(&b'{');
        if braced {
            j = skip_ws(b, j + 1);
        }
        if b.get(j) != Some(&b'\\') {
            i = j.max(at + 1);
            continue;
        }
        let name_start = j + 1;
        let mut k = name_start;
        while k < b.len() && b[k].is_ascii_alphabetic() {
            k += 1;
        }
        let name = &src[name_start..k];
        if braced {
            k = skip_ws(b, k);
            if b.get(k) == Some(&b'}') {
                k += 1;
            }
        }
        k = skip_ws(b, k);
        let has_params = if kw.ends_with("def") {
            b.get(k) == Some(&b'#')
        } else {
            b.get(k) == Some(&b'[')
        };
        if has_params || name.is_empty() {
            i = k.max(at + 1);
            continue;
        }
        if b.get(k) == Some(&b'{')
            && let Some(e) = group_end(b, k)
        {
            out.insert(name.to_owned(), src[k + 1..e - 1].to_owned());
            i = e;
            continue;
        }
        i = k.max(at + 1);
    }
    out
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn group_end(b: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = start;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 1,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}
