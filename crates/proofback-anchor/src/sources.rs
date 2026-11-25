//! Project sources held in memory, with cached normalisation.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use proofback_tex::{Folded, Normalized, Options, fold_str, normalize};

/// A source file and its derived views.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub text: String,
    pub normalized: Normalized,
    pub folded: Folded,
    /// `folded` as a string, for fast substring search.
    pub folded_text: String,
    /// Character index of each byte in `folded_text` (valid at char starts).
    folded_byte_char: Vec<u32>,
    line_starts: Vec<usize>,
}

impl SourceFile {
    fn new(text: String, opts: &Options) -> Self {
        let normalized = normalize(&text, opts);
        let folded = fold_str(normalized.chars().iter().copied());
        let mut line_starts = vec![0];
        line_starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        let folded_text: String = folded.chars.iter().collect();
        let mut folded_byte_char = vec![0u32; folded_text.len() + 1];
        for (ci, (bi, _)) in folded_text.char_indices().enumerate() {
            folded_byte_char[bi] = ci as u32;
        }
        folded_byte_char[folded_text.len()] = folded.chars.len() as u32;
        Self {
            text,
            normalized,
            folded,
            folded_text,
            folded_byte_char,
            line_starts,
        }
    }

    /// All occurrences of `needle` (already folded) as folded character ranges.
    pub fn find_folded(&self, needle: &str) -> Vec<Range<usize>> {
        if needle.is_empty() {
            return Vec::new();
        }
        self.folded_text
            .match_indices(needle)
            .map(|(b, m)| {
                self.folded_byte_char[b] as usize..self.folded_byte_char[b + m.len()] as usize
            })
            .collect()
    }

    /// Number of lines.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Byte range of 1-based line `line`, without the line break.
    pub fn line_range(&self, line: usize) -> Option<Range<usize>> {
        let start = *self.line_starts.get(line.checked_sub(1)?)?;
        let end = self
            .line_starts
            .get(line)
            .map_or(self.text.len(), |&n| n - 1);
        let end = if end > start && self.text.as_bytes().get(end - 1) == Some(&b'\r') {
            end - 1
        } else {
            end
        };
        Some(start..end.max(start))
    }

    /// 1-based line and column (Unicode code points) of a byte offset.
    pub fn line_col(&self, offset: usize) -> (u32, u32) {
        let offset = offset.min(self.text.len());
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let start = self.line_starts[line_idx];
        let col = self.text[start..offset].chars().count() + 1;
        (line_idx as u32 + 1, col as u32)
    }

    /// Folded character range covering source bytes `range`.
    pub fn folded_range_for_source(&self, range: Range<usize>) -> Option<Range<usize>> {
        let chars = self.normalized.chars_for_source(range)?;
        let start = self.folded.origin.iter().position(|&o| o >= chars.start)?;
        let end = self.folded.origin.iter().rposition(|&o| o < chars.end)? + 1;
        (start < end).then_some(start..end)
    }

    /// Source byte range of a folded character range.
    pub fn source_range_for_folded(&self, range: Range<usize>) -> Option<Range<usize>> {
        if range.start >= range.end || range.end > self.folded.origin.len() {
            return None;
        }
        let first = self.folded.origin[range.start];
        let last = self.folded.origin[range.end - 1];
        self.normalized.source_range(first..last + 1)
    }
}

/// All `.tex` sources of a project, keyed by path relative to the project root
/// with `/` separators.
#[derive(Debug, Clone, Default)]
pub struct Sources {
    files: BTreeMap<String, SourceFile>,
    opts: Options,
}

impl Sources {
    /// Build from raw file contents. Macros are collected from every file.
    pub fn new(files: impl IntoIterator<Item = (String, String)>) -> Self {
        let raw: Vec<(String, String)> = files.into_iter().collect();
        let mut macros = HashMap::new();
        for (_, text) in &raw {
            macros.extend(proofback_tex::collect_macros(text));
        }
        let opts = Options { macros };
        let files = raw
            .into_iter()
            .map(|(k, v)| (k, SourceFile::new(v, &opts)))
            .collect();
        Self { files, opts }
    }

    pub fn get(&self, rel: &str) -> Option<&SourceFile> {
        self.files.get(rel)
    }

    pub fn files(&self) -> impl Iterator<Item = (&str, &SourceFile)> {
        self.files.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn options(&self) -> &Options {
        &self.opts
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_columns() {
        let s = Sources::new([("a.tex".to_owned(), "one\r\ntwo é x\nthree".to_owned())]);
        let f = s.get("a.tex").unwrap();
        assert_eq!(f.line_count(), 3);
        assert_eq!(&f.text[f.line_range(1).unwrap()], "one");
        assert_eq!(&f.text[f.line_range(2).unwrap()], "two é x");
        assert_eq!(&f.text[f.line_range(3).unwrap()], "three");
        let x = f.text.find(" x").unwrap() + 1;
        assert_eq!(f.line_col(x), (2, 7));
        assert!(f.line_range(4).is_none());
    }

    #[test]
    fn folded_and_source_ranges_round_trip() {
        let s = Sources::new([("a.tex".to_owned(), "The \\emph{Robust} way".to_owned())]);
        let f = s.get("a.tex").unwrap();
        let start = f.text.find("Robust").unwrap();
        let fr = f.folded_range_for_source(start..start + 6).unwrap();
        assert_eq!(
            f.folded.chars[fr.clone()].iter().collect::<String>(),
            "robust"
        );
        assert_eq!(f.source_range_for_folded(fr).unwrap(), start..start + 6);
    }
}
