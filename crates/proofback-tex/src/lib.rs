//! TeX-aware normaliser with an offset map back to the source.
//!
//! [`normalize`] turns LaTeX source into an approximation of the text a reader sees
//! in the PDF, and records for every output character the byte range of the source
//! it came from. proofback aligns the text a reviewer marked in the PDF against this
//! output and maps the hit back to an exact source range.
//!
//! This is deliberately **not** a TeX engine. It only needs to be good enough for
//! fuzzy alignment inside the small window that `SyncTeX` has already found.

mod fold;
mod macros;

use std::collections::HashMap;
use std::ops::Range;

use unicode_normalization::UnicodeNormalization;

pub use fold::{Folded, fold_char, fold_str};
pub use macros::collect_macros;

/// Stands in for content whose rendered text cannot be known from the source:
/// inline maths, references, citations, unknown macros.
pub const PLACEHOLDER: char = '\u{FFFC}';

/// Normalisation options.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Parameterless user macros to expand: name (without backslash) to body.
    pub macros: HashMap<String, String>,
}

/// Kind of a non-paragraph block in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Figure,
    Table,
    DisplayMath,
    Verbatim,
}

/// A figure, table, display-maths or verbatim block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    /// Source byte range of the whole block.
    pub source: Range<usize>,
}

/// Normalised text with a per-character map to source byte ranges.
#[derive(Debug, Clone, Default)]
pub struct Normalized {
    chars: Vec<char>,
    spans: Vec<(u32, u32)>,
    blocks: Vec<Block>,
}

impl Normalized {
    /// The normalised text.
    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    /// The normalised characters.
    pub fn chars(&self) -> &[char] {
        &self.chars
    }

    /// Number of output characters.
    pub fn len(&self) -> usize {
        self.chars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    /// Source byte range of output character `i`.
    pub fn span(&self, i: usize) -> Range<usize> {
        let (s, e) = self.spans[i];
        s as usize..e as usize
    }

    /// Source byte range covering output characters `range` (character indices).
    ///
    /// Returns `None` for an empty or out-of-bounds range.
    pub fn source_range(&self, range: Range<usize>) -> Option<Range<usize>> {
        if range.start >= range.end || range.end > self.chars.len() {
            return None;
        }
        let start = self.spans[range.start..range.end]
            .iter()
            .map(|s| s.0)
            .min()?;
        let end = self.spans[range.start..range.end]
            .iter()
            .map(|s| s.1)
            .max()?;
        Some(start as usize..end as usize)
    }

    /// Output character indices whose source span intersects `source`.
    pub fn chars_for_source(&self, source: Range<usize>) -> Option<Range<usize>> {
        let first = self
            .spans
            .iter()
            .position(|&(s, e)| (e as usize) > source.start && (s as usize) < source.end)?;
        let last = self
            .spans
            .iter()
            .rposition(|&(s, e)| (e as usize) > source.start && (s as usize) < source.end)?;
        Some(first..last + 1)
    }

    /// Figure, table, display-maths and verbatim blocks, in source order.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }
}

/// Normalise a complete source file.
pub fn normalize(src: &str, opts: &Options) -> Normalized {
    let mut n = Normalizer {
        src,
        opts,
        out: Normalized::default(),
        depth: 0,
    };
    n.run(0, src.len(), None);
    n.trim_trailing_space();
    n.out
}

const MAX_MACRO_DEPTH: u32 = 8;

struct Normalizer<'a> {
    src: &'a str,
    opts: &'a Options,
    out: Normalized,
    depth: u32,
}

/// How a known command is treated.
enum Cmd {
    /// Drop the command and `n` mandatory arguments (optional arguments too).
    Drop(usize),
    /// Replace the command and `n` mandatory arguments with one placeholder.
    Placeholder(usize),
    /// Drop the command name and optional arguments; keep the mandatory argument text.
    Keep,
    /// Drop the first mandatory argument, keep the rest as text (`\textcolor`, `\href`).
    DropFirst,
    /// Emit fixed text.
    Text(&'static str),
    /// Emit a single space.
    Space,
    /// Emit nothing.
    Nothing,
}

fn command(name: &str) -> Option<Cmd> {
    Some(match name {
        "label" | "index" | "vspace" | "hspace" | "usepackage" | "RequirePackage"
        | "documentclass" | "bibliographystyle" | "bibliography" | "addbibresource" | "input"
        | "include" | "subfile" | "includegraphics" | "includepdf" | "pagestyle"
        | "thispagestyle" | "color" | "hypersetup" | "graphicspath" | "nocite" | "end" | "todo"
        | "pagenumbering" | "includeonly" | "tableofcontents" | "listoffigures"
        | "listoftables" | "geometry" | "linespread" | "fontsize" | "settowidth" => match name {
            "tableofcontents" | "listoffigures" | "listoftables" => Cmd::Drop(0),
            "fontsize" | "settowidth" => Cmd::Drop(2),
            _ => Cmd::Drop(1),
        },
        "setlength" | "addtolength" | "setcounter" | "addtocounter" => Cmd::Drop(2),
        "newenvironment" | "renewenvironment" => Cmd::Drop(3),
        "ref" | "eqref" | "cref" | "Cref" | "autoref" | "pageref" | "nameref" | "vref" | "cite"
        | "citep" | "citet" | "citealp" | "citealt" | "citeauthor" | "citeyear" | "parencite"
        | "textcite" | "autocite" | "footcite" | "fullcite" | "smartcite" | "supercite" | "gls"
        | "glspl" | "acrshort" | "acrlong" | "acrfull" | "ac" | "acp" => Cmd::Placeholder(1),
        "emph" | "textbf" | "textit" | "texttt" | "textsc" | "textrm" | "textsf" | "textup"
        | "textsl" | "textmd" | "textnormal" | "underline" | "uline" | "mbox" | "hbox" | "text"
        | "textsuperscript" | "textsubscript" | "section" | "subsection" | "subsubsection"
        | "chapter" | "paragraph" | "subparagraph" | "part" | "caption" | "captionof"
        | "footnote" | "footnotetext" | "title" | "author" | "date" | "enquote" | "makebox"
        | "framebox" | "fbox" | "centerline" | "noindent" | "item" | "url" | "nolinkurl"
        | "path" => match name {
            "noindent" => Cmd::Nothing,
            "item" => Cmd::Space,
            _ => Cmd::Keep,
        },
        "textcolor" | "href" | "colorbox" => Cmd::DropFirst,
        "ldots" | "dots" | "textellipsis" => Cmd::Text("…"),
        "LaTeX" => Cmd::Text("LaTeX"),
        "TeX" => Cmd::Text("TeX"),
        "LaTeXe" => Cmd::Text("LaTeX2ε"),
        "S" => Cmd::Text("§"),
        "P" => Cmd::Text("¶"),
        "copyright" | "textcopyright" => Cmd::Text("©"),
        "textregistered" => Cmd::Text("®"),
        "texttrademark" => Cmd::Text("™"),
        "textendash" => Cmd::Text("–"),
        "textemdash" => Cmd::Text("—"),
        "textbullet" => Cmd::Text("•"),
        "textdegree" => Cmd::Text("°"),
        "i" => Cmd::Text("ı"),
        "j" => Cmd::Text("ȷ"),
        "ss" => Cmd::Text("ß"),
        "ae" => Cmd::Text("æ"),
        "AE" => Cmd::Text("Æ"),
        "oe" => Cmd::Text("œ"),
        "OE" => Cmd::Text("Œ"),
        "o" => Cmd::Text("ø"),
        "O" => Cmd::Text("Ø"),
        "aa" => Cmd::Text("å"),
        "AA" => Cmd::Text("Å"),
        "l" => Cmd::Text("ł"),
        "L" => Cmd::Text("Ł"),
        "%" | "&" | "$" | "#" | "_" | "{" | "}" => Cmd::Text(match name {
            "%" => "%",
            "&" => "&",
            "$" => "$",
            "#" => "#",
            "_" => "_",
            "{" => "{",
            _ => "}",
        }),
        " " | "," | ";" | ":" | "quad" | "qquad" | "\\" | "newline" | "linebreak" | "par"
        | "smallskip" | "medskip" | "bigskip" | "newpage" | "clearpage" | "cleardoublepage"
        | "pagebreak" | "hfill" | "vfill" | "enspace" | "thinspace" => Cmd::Space,
        "-" | "/" | "!" | "maketitle" | "centering" | "raggedright" | "raggedleft" | "small"
        | "footnotesize" | "scriptsize" | "tiny" | "large" | "Large" | "LARGE" | "huge"
        | "Huge" | "normalsize" | "bfseries" | "itshape" | "ttfamily" | "rmfamily" | "sffamily"
        | "scshape" | "upshape" | "em" | "bf" | "it" | "tt" | "rm" | "sf" | "sc" | "relax"
        | "protect" | "appendix" | "frontmatter" | "mainmatter" | "backmatter" | "hline"
        | "toprule" | "midrule" | "bottomrule" | "cline" | "cmidrule" | "nopagebreak"
        | "allowbreak" | "xspace" | "selectlanguage" | "phantomsection" | "printbibliography" => {
            match name {
                "cline" | "cmidrule" | "selectlanguage" => Cmd::Drop(1),
                _ => Cmd::Nothing,
            }
        }
        _ => return None,
    })
}

fn accent(name: &str) -> Option<char> {
    Some(match name {
        "'" => '\u{0301}',
        "`" => '\u{0300}',
        "^" => '\u{0302}',
        "\"" => '\u{0308}',
        "~" => '\u{0303}',
        "=" => '\u{0304}',
        "." => '\u{0307}',
        "u" => '\u{0306}',
        "v" => '\u{030C}',
        "H" => '\u{030B}',
        "c" => '\u{0327}',
        "k" => '\u{0328}',
        "r" => '\u{030A}',
        "d" => '\u{0323}',
        "b" => '\u{0331}',
        _ => return None,
    })
}

fn env_block(name: &str) -> Option<BlockKind> {
    let base = name.trim_end_matches('*');
    Some(match base {
        "figure" | "wrapfigure" | "subfigure" => BlockKind::Figure,
        "table" | "wraptable" => BlockKind::Table,
        "equation" | "align" | "alignat" | "gather" | "multline" | "eqnarray" | "displaymath"
        | "flalign" | "math" => BlockKind::DisplayMath,
        "verbatim" | "Verbatim" | "lstlisting" | "minted" | "alltt" => BlockKind::Verbatim,
        _ => return None,
    })
}

impl Normalizer<'_> {
    fn bytes(&self) -> &[u8] {
        self.src.as_bytes()
    }

    fn push(&mut self, c: char, span: Range<usize>) {
        if c == ' ' {
            match self.out.chars.last() {
                None | Some(' ') => return,
                _ => {}
            }
        }
        self.out.chars.push(c);
        self.out.spans.push((span.start as u32, span.end as u32));
    }

    fn push_str(&mut self, s: &str, span: Range<usize>) {
        for c in s.chars() {
            self.push(c, span.clone());
        }
    }

    fn trim_trailing_space(&mut self) {
        while self.out.chars.last() == Some(&' ') {
            self.out.chars.pop();
            self.out.spans.pop();
        }
    }

    /// Process `src[start..end]`. `stop` ends early at a matching `\end{stop}`.
    fn run(&mut self, start: usize, end: usize, stop: Option<&str>) -> usize {
        let mut i = start;
        while i < end {
            let b = self.bytes()[i];
            match b {
                b'%' => i = self.skip_comment(i, end),
                b'\\' => {
                    if let Some(stop) = stop
                        && self.src[i..end].starts_with("\\end")
                    {
                        let (name, after) = self.peek_env_name(i + 4, end);
                        if name.as_deref() == Some(stop) {
                            return after;
                        }
                    }
                    i = self.control(i, end);
                }
                b'{' | b'}' => i += 1,
                b'$' => i = self.math_dollar(i, end),
                b'~' => {
                    self.push(' ', i..i + 1);
                    i += 1;
                }
                b'&' => {
                    self.push(' ', i..i + 1);
                    i += 1;
                }
                b'-' => {
                    let n = self.bytes()[i..end]
                        .iter()
                        .take_while(|&&c| c == b'-')
                        .count();
                    let (c, len) = match n {
                        1 => ('-', 1),
                        2 => ('–', 2),
                        _ => ('—', 3),
                    };
                    self.push(c, i..i + len);
                    i += len;
                }
                b'`' => {
                    if self.bytes().get(i + 1) == Some(&b'`') && i + 1 < end {
                        self.push('“', i..i + 2);
                        i += 2;
                    } else {
                        self.push('‘', i..i + 1);
                        i += 1;
                    }
                }
                b'\'' => {
                    if self.bytes().get(i + 1) == Some(&b'\'') && i + 1 < end {
                        self.push('”', i..i + 2);
                        i += 2;
                    } else {
                        self.push('’', i..i + 1);
                        i += 1;
                    }
                }
                b' ' | b'\t' | b'\r' | b'\n' => {
                    let j = self.skip_ws(i, end);
                    self.push(' ', i..j);
                    i = j;
                }
                _ => {
                    let c = self.src[i..].chars().next().unwrap_or('\u{FFFD}');
                    let len = c.len_utf8();
                    self.push(c, i..i + len);
                    i += len;
                }
            }
        }
        end
    }

    fn skip_ws(&self, mut i: usize, end: usize) -> usize {
        while i < end && matches!(self.bytes()[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        i
    }

    fn skip_comment(&self, mut i: usize, end: usize) -> usize {
        while i < end && self.bytes()[i] != b'\n' {
            i += 1;
        }
        // The newline after a comment is eaten by TeX, and so is leading space on
        // the next line.
        if i < end {
            i += 1;
        }
        while i < end && matches!(self.bytes()[i], b' ' | b'\t') {
            i += 1;
        }
        i
    }

    /// Read a control sequence name starting after the backslash at `i`.
    fn read_name(&self, i: usize, end: usize) -> (String, usize) {
        let start = i + 1;
        if start >= end {
            return (String::new(), start);
        }
        let mut j = start;
        while j < end && self.bytes()[j].is_ascii_alphabetic() {
            j += 1;
        }
        if j == start {
            // Control symbol: exactly one character.
            let c = self.src[start..].chars().next().unwrap_or(' ');
            let len = c.len_utf8();
            return (c.to_string(), (start + len).min(end));
        }
        (self.src[start..j].to_owned(), j)
    }

    fn skip_star(&self, i: usize, end: usize) -> usize {
        if i < end && self.bytes()[i] == b'*' {
            i + 1
        } else {
            i
        }
    }

    /// Skip whitespace (not a blank line) between a command and its arguments.
    fn skip_arg_ws(&self, mut i: usize, end: usize) -> usize {
        let mut newlines = 0;
        while i < end {
            match self.bytes()[i] {
                b' ' | b'\t' | b'\r' => i += 1,
                b'\n' if newlines == 0 => {
                    newlines += 1;
                    i += 1;
                }
                _ => break,
            }
        }
        i
    }

    /// End of a balanced group starting at `i` (which must be `open`).
    fn group_end(&self, i: usize, end: usize, open: u8, close: u8) -> Option<usize> {
        let mut depth = 0usize;
        let mut j = i;
        while j < end {
            match self.bytes()[j] {
                b'\\' => j += 1,
                c if c == open => depth += 1,
                c if c == close => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(j + 1);
                    }
                }
                b'%' if open == b'{' => {
                    while j < end && self.bytes()[j] != b'\n' {
                        j += 1;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        None
    }

    fn skip_optional_args(&self, mut i: usize, end: usize) -> usize {
        loop {
            let k = self.skip_arg_ws(i, end);
            if k < end && self.bytes()[k] == b'[' {
                match self.group_end(k, end, b'[', b']') {
                    Some(e) => i = e,
                    None => return i,
                }
            } else {
                return i;
            }
        }
    }

    /// Skip one mandatory argument (a braced group or a single token).
    fn skip_arg(&self, i: usize, end: usize) -> usize {
        let k = self.skip_arg_ws(i, end);
        if k >= end {
            return k;
        }
        match self.bytes()[k] {
            b'{' => self.group_end(k, end, b'{', b'}').unwrap_or(end),
            b'\\' => self.read_name(k, end).1,
            _ => k + self.src[k..].chars().next().map_or(1, char::len_utf8),
        }
    }

    /// Byte range of the content of the next braced argument, if there is one.
    fn arg_content(&self, i: usize, end: usize) -> Option<(Range<usize>, usize)> {
        let k = self.skip_arg_ws(i, end);
        if k < end && self.bytes()[k] == b'{' {
            let e = self.group_end(k, end, b'{', b'}')?;
            Some((k + 1..e - 1, e))
        } else {
            None
        }
    }

    fn peek_env_name(&self, i: usize, end: usize) -> (Option<String>, usize) {
        match self.arg_content(i, end) {
            Some((r, after)) => (Some(self.src[r].trim().to_owned()), after),
            None => (None, i),
        }
    }

    fn control(&mut self, i: usize, end: usize) -> usize {
        let (name, mut j) = self.read_name(i, end);
        if name.is_empty() {
            return j;
        }
        let is_word = name.as_bytes()[0].is_ascii_alphabetic();

        // Spaces after a control word are not typeset.
        let eat_space = |n: &Self, j: usize| if is_word { n.skip_arg_ws(j, end) } else { j };

        if let Some(mark) = accent(&name) {
            return self.accent(i, j, end, mark);
        }
        match name.as_str() {
            "begin" => return self.begin_env(i, j, end),
            "verb" => return self.verb(i, j, end),
            "(" => return self.math_until(i, j, end, "\\)", false),
            "[" => return self.math_until(i, j, end, "\\]", true),
            "newcommand" | "renewcommand" | "providecommand" | "DeclareRobustCommand" => {
                let mut k = self.skip_star(j, end);
                k = self.skip_arg(k, end);
                k = self.skip_optional_args(k, end);
                return self.skip_arg(k, end);
            }
            "def" | "gdef" | "edef" | "xdef" => {
                let mut k = self.skip_arg_ws(j, end);
                if k < end && self.bytes()[k] == b'\\' {
                    k = self.read_name(k, end).1;
                }
                while k < end && self.bytes()[k] != b'{' {
                    k += 1;
                }
                return self.skip_arg(k, end);
            }
            _ => {}
        }
        if let Some(cmd) = command(&name) {
            j = self.skip_star(j, end);
            return match cmd {
                Cmd::Drop(n) => {
                    let mut k = self.skip_optional_args(j, end);
                    for _ in 0..n {
                        k = self.skip_arg(k, end);
                        k = self.skip_optional_args(k, end);
                    }
                    k
                }
                Cmd::Placeholder(n) => {
                    let mut k = self.skip_optional_args(j, end);
                    for _ in 0..n {
                        k = self.skip_arg(k, end);
                        k = self.skip_optional_args(k, end);
                    }
                    self.push(PLACEHOLDER, i..k);
                    k
                }
                Cmd::Keep => self.skip_optional_args(j, end),
                Cmd::DropFirst => {
                    let k = self.skip_optional_args(j, end);
                    self.skip_arg(k, end)
                }
                Cmd::Text(t) => {
                    self.push_str(t, i..j);
                    eat_space(self, j)
                }
                Cmd::Space => {
                    let k = if name == "\\" {
                        self.skip_optional_args(self.skip_star(j, end), end)
                    } else {
                        j
                    };
                    self.push(' ', i..k);
                    eat_space(self, k)
                }
                Cmd::Nothing => eat_space(self, j),
            };
        }
        if let Some(body) = self.opts.macros.get(&name)
            && self.depth < MAX_MACRO_DEPTH
        {
            let body = body.clone();
            let inner = {
                let mut sub = Normalizer {
                    src: &body,
                    opts: self.opts,
                    out: Normalized::default(),
                    depth: self.depth + 1,
                };
                sub.run(0, body.len(), None);
                sub.out.chars
            };
            for c in inner {
                self.push(c, i..j);
            }
            return eat_space(self, j);
        }
        // Unknown command: its rendering is unknowable from the source.
        self.push(PLACEHOLDER, i..j);
        eat_space(self, j)
    }

    fn accent(&mut self, i: usize, j: usize, end: usize, mark: char) -> usize {
        let k = if name_is_letter_accent(self.src, i) {
            self.skip_arg_ws(j, end)
        } else {
            j
        };
        let (base, after) = if k < end && self.bytes()[k] == b'{' {
            let (inner_end, e) = match self.group_end(k, end, b'{', b'}') {
                Some(e) => (e - 1, e),
                None => (end, end),
            };
            let inner = self.src[k + 1..inner_end.max(k + 1)].trim();
            let base = match inner {
                "\\i" => Some('ı'),
                "\\j" => Some('ȷ'),
                _ => inner.chars().next(),
            };
            (base, e)
        } else if k < end
            && self.src[k..].starts_with("\\i")
            && !self.bytes().get(k + 2).is_some_and(u8::is_ascii_alphabetic)
        {
            (Some('ı'), k + 2)
        } else if k < end {
            let c = self.src[k..].chars().next();
            (c, k + c.map_or(1, char::len_utf8))
        } else {
            (None, k)
        };
        if let Some(base) = base {
            let base = match base {
                'ı' => 'i',
                'ȷ' => 'j',
                c => c,
            };
            let composed: String = [base, mark].iter().collect::<String>().nfc().collect();
            for c in composed.chars() {
                self.push(c, i..after);
            }
        }
        after
    }

    fn begin_env(&mut self, i: usize, j: usize, end: usize) -> usize {
        let Some((name_range, mut k)) = self.arg_content(j, end) else {
            return j;
        };
        let name = self.src[name_range].trim().to_owned();
        let base = name.trim_end_matches('*');
        if base == "comment" {
            return self.find_env_end(k, end, &name).map_or(end, |r| r.end);
        }
        // Arguments that are not typeset.
        k = self.skip_optional_args(k, end);
        match base {
            "tabular" | "array" | "longtable" | "supertabular" => k = self.skip_arg(k, end),
            "tabularx" | "tabulary" | "tabular*" => {
                k = self.skip_arg(k, end);
                k = self.skip_arg(k, end);
            }
            "minipage" | "wrapfigure" | "wraptable" => {
                k = self.skip_arg(k, end);
                if base != "minipage" {
                    k = self.skip_arg(k, end);
                }
            }
            "minted" => k = self.skip_arg(k, end),
            "thebibliography" => k = self.skip_arg(k, end),
            _ => {}
        }
        k = self.skip_optional_args(k, end);
        match env_block(&name) {
            Some(BlockKind::Verbatim) => {
                let body_end = self.find_env_end(k, end, &name);
                let (content_end, after) = body_end.map_or((end, end), |r| (r.start, r.end));
                self.push(' ', i..k);
                for (off, c) in self.src[k..content_end].char_indices() {
                    let p = k + off;
                    let len = c.len_utf8();
                    let c = if c.is_whitespace() { ' ' } else { c };
                    self.push(c, p..p + len);
                }
                self.push(' ', content_end..after);
                self.out.blocks.push(Block {
                    kind: BlockKind::Verbatim,
                    source: i..after,
                });
                after
            }
            Some(BlockKind::DisplayMath) => {
                let after = self.find_env_end(k, end, &name).map_or(end, |r| r.end);
                self.push(' ', i..after);
                self.out.blocks.push(Block {
                    kind: BlockKind::DisplayMath,
                    source: i..after,
                });
                after
            }
            Some(kind) => {
                self.push(' ', i..k);
                let after = self.run(k, end, Some(&name));
                self.push(' ', after..after);
                self.out.blocks.push(Block {
                    kind,
                    source: i..after,
                });
                after
            }
            None => k,
        }
    }

    /// Byte range of `\end{name}` after `from`.
    fn find_env_end(&self, from: usize, end: usize, name: &str) -> Option<Range<usize>> {
        let needle = format!("\\end{{{name}}}");
        self.src[from..end]
            .find(&needle)
            .map(|p| from + p..from + p + needle.len())
    }

    fn verb(&mut self, i: usize, j: usize, end: usize) -> usize {
        let j = self.skip_star(j, end);
        let Some(delim) = self.src[j..end].chars().next() else {
            return j;
        };
        let start = j + delim.len_utf8();
        let Some(close) = self.src[start..end].find(delim) else {
            return end;
        };
        let content_end = start + close;
        for (off, c) in self.src[start..content_end].char_indices() {
            let p = start + off;
            self.push(c, p..p + c.len_utf8());
        }
        let _ = i;
        content_end + delim.len_utf8()
    }

    fn math_dollar(&mut self, i: usize, end: usize) -> usize {
        let display = self.bytes().get(i + 1) == Some(&b'$') && i + 1 < end;
        if display {
            let start = i + 2;
            let after = self.src[start..end]
                .find("$$")
                .map_or(end, |p| start + p + 2);
            self.push(' ', i..after);
            self.out.blocks.push(Block {
                kind: BlockKind::DisplayMath,
                source: i..after,
            });
            return after;
        }
        let mut j = i + 1;
        while j < end {
            match self.bytes()[j] {
                b'\\' => j += 1 + self.src[j + 1..].chars().next().map_or(0, char::len_utf8),
                b'$' => break,
                _ => j += 1,
            }
        }
        let after = (j + 1).min(end);
        self.push(PLACEHOLDER, i..after);
        after
    }

    fn math_until(&mut self, i: usize, j: usize, end: usize, close: &str, display: bool) -> usize {
        let after = self.src[j..end]
            .find(close)
            .map_or(end, |p| j + p + close.len());
        if display {
            self.push(' ', i..after);
            self.out.blocks.push(Block {
                kind: BlockKind::DisplayMath,
                source: i..after,
            });
        } else {
            self.push(PLACEHOLDER, i..after);
        }
        after
    }
}

/// Letter accents (`\c`, `\u`, `\v`, ...) may be followed by a space before their
/// argument; symbol accents (`\'`, `\"`) take the next character directly.
fn name_is_letter_accent(src: &str, i: usize) -> bool {
    src.as_bytes()
        .get(i + 1)
        .is_some_and(u8::is_ascii_alphabetic)
}

#[cfg(test)]
mod tests;
