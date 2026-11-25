//! Character folding used on both sides of an alignment (PDF text and normalised
//! TeX), so that typographic differences do not count as mismatches.

/// A folded string with a map from each folded character to the index of the
/// original character it came from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folded {
    pub chars: Vec<char>,
    pub origin: Vec<usize>,
}

/// Fold one character into zero or more comparison characters.
///
/// Lower-cases, unifies quotes, dashes and spaces, and expands ligatures.
pub fn fold_char(c: char, out: &mut Vec<char>) {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' | '`' | '´' => {
            out.push('\'');
        }
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' | '«' | '»' => {
            out.push('"');
        }
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
        | '\u{2212}' => out.push('-'),
        '\u{00A0}' | '\u{2002}'..='\u{200A}' | '\u{202F}' | '\t' | '\n' | '\r' => out.push(' '),
        '\u{00AD}' | '\u{200B}' | '\u{FEFF}' => {}
        '\u{FB00}' => out.extend(['f', 'f']),
        '\u{FB01}' => out.extend(['f', 'i']),
        '\u{FB02}' => out.extend(['f', 'l']),
        '\u{FB03}' => out.extend(['f', 'f', 'i']),
        '\u{FB04}' => out.extend(['f', 'f', 'l']),
        '\u{FB05}' | '\u{FB06}' => out.extend(['s', 't']),
        '…' => out.extend(['.', '.', '.']),
        c => out.extend(c.to_lowercase()),
    }
}

/// Fold a sequence of characters, collapsing runs of spaces.
pub fn fold_str<I: IntoIterator<Item = char>>(chars: I) -> Folded {
    let mut f = Folded::default();
    let mut tmp = Vec::with_capacity(3);
    for (i, c) in chars.into_iter().enumerate() {
        tmp.clear();
        fold_char(c, &mut tmp);
        for &fc in &tmp {
            if fc == ' ' && matches!(f.chars.last(), None | Some(' ')) {
                continue;
            }
            f.chars.push(fc);
            f.origin.push(i);
        }
    }
    f
}
