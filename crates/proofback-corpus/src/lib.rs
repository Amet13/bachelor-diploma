//! Synthetic accuracy corpus.
//!
//! [`generate`] places annotations of every type on text whose source location is
//! known, writing them the way different PDF apps do (quad order, missing notes,
//! padding). [`evaluate`] runs the real import on the annotated PDF and compares
//! every item with the ground truth.
//!
//! Ground truth is only taken from spans that occur exactly once in both the PDF
//! text and the normalised source, so it is unambiguous.

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use proofback_anchor::{Input, Sources, import};
use proofback_model::{Confidence, Item, Origin};
use proofback_pdf::{AnnotationSpec, Limits, PageText, TextExtractor, extract, write_annotations};
use proofback_synctex::SyncTex;
use proofback_tex::fold_str;
use serde::{Deserialize, Serialize};

/// A fixture project: sources, SyncTeX and the compiled PDF.
#[derive(Debug)]
pub struct Fixture {
    pub name: String,
    pub root: PathBuf,
    pub sources: Sources,
    pub synctex: SyncTex,
    pub pdf: Vec<u8>,
}

impl Fixture {
    pub fn load(root: &Path) -> std::io::Result<Self> {
        let mut files = Vec::new();
        collect_tex(root, root, &mut files)?;
        let synctex = SyncTex::open(
            root.join("main.synctex.gz"),
            proofback_synctex::Limits::default(),
        )
        .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(Self {
            name: root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            root: root.to_path_buf(),
            sources: Sources::new(files),
            synctex,
            pdf: std::fs::read(root.join("main.pdf"))?,
        })
    }
}

fn collect_tex(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::path);
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            collect_tex(root, &p, out)?;
        } else if p.extension().is_some_and(|x| x == "tex") {
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, std::fs::read_to_string(&p)?));
        }
    }
    Ok(())
}

/// The annotation types the corpus exercises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Case {
    Highlight,
    HighlightNote,
    Delete,
    Replace,
    Question,
    Squiggly,
    Insert,
    StickyNote,
}

impl Case {
    pub const ALL: [Case; 8] = [
        Case::Highlight,
        Case::HighlightNote,
        Case::Delete,
        Case::Replace,
        Case::Question,
        Case::Squiggly,
        Case::Insert,
        Case::StickyNote,
    ];

    /// Cases whose correct answer is an exact character range or insertion point.
    pub fn expects_exact(self) -> bool {
        !matches!(self, Case::StickyNote)
    }
}

/// Ground truth for one generated annotation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Truth {
    pub nm: String,
    pub case: Case,
    pub file: String,
    /// Expected byte range (empty for insertion points).
    pub range: Range<usize>,
    /// Lines that count as correct for line-level answers.
    pub lines: Range<u32>,
}

/// Tiny deterministic generator (xorshift64*).
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
}

struct PageIndex {
    text: PageText,
    folded: String,
}

/// Generate up to `count` annotations on a fixture. Returns the annotated PDF and
/// the ground truth.
pub fn generate(
    fx: &Fixture,
    pdfium: &pdfium_render::prelude::Pdfium,
    count: usize,
    seed: u64,
) -> Result<(Vec<u8>, Vec<Truth>), String> {
    let ex = TextExtractor::open(pdfium, &fx.pdf).map_err(|e| e.to_string())?;
    let pages: Vec<PageIndex> = (1..=ex.page_count())
        .map(|p| {
            let text = ex.page(p).map_err(|e| e.to_string())?;
            let f = fold_str(text.chars.iter().map(|c| c.c));
            Ok(PageIndex {
                folded: f.chars.iter().collect(),
                text,
            })
        })
        .collect::<Result<_, String>>()?;
    let all_pdf: String = pages
        .iter()
        .map(|p| p.folded.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    let mut rng = Rng::new(seed);
    let mut specs = Vec::new();
    let mut truths = Vec::new();
    let mut used: Vec<(u32, Range<usize>)> = Vec::new();
    let mut attempts = 0;
    while truths.len() < count && attempts < count * 200 {
        attempts += 1;
        let case = Case::ALL[truths.len() % Case::ALL.len()];
        let pi = rng.below(pages.len());
        let page = &pages[pi];
        let page_no = pi as u32 + 1;
        let chars = &page.text.chars;
        if chars.len() < 50 {
            continue;
        }
        // A span of 1-4 whole words, possibly across a line break for long spans.
        let start = rng.below(chars.len() - 1);
        if !chars[start].c.is_alphabetic() || (start > 0 && chars[start - 1].c.is_alphanumeric()) {
            continue;
        }
        let words = 1 + rng.below(4);
        let mut end = start;
        let mut seen = 0;
        while end < chars.len() {
            if !chars[end].c.is_alphanumeric() {
                if chars[end].c == ' ' || chars[end].c == '\r' || chars[end].c == '\n' {
                    seen += 1;
                    if seen == words {
                        break;
                    }
                } else {
                    break;
                }
            }
            end += 1;
        }
        let span: String = chars[start..end].iter().map(|c| c.c).collect();
        if span.contains(['\u{2}', '-', '\u{FFFE}'])
            || span.chars().filter(|c| c.is_alphabetic()).count() < 4
        {
            continue;
        }
        if used
            .iter()
            .any(|(p, r)| *p == page_no && r.start < end + 2 && start < r.end + 2)
        {
            continue;
        }
        let folded: String = trim(&fold_str(span.chars()).chars.iter().collect::<String>());
        if folded.chars().count() < 5 || all_pdf.matches(&folded).count() != 1 {
            continue;
        }
        let mut found = Vec::new();
        for (f, src) in fx.sources.files() {
            for r in src.find_folded(&folded) {
                found.push((f.to_owned(), r));
            }
        }
        if found.len() != 1 {
            continue;
        }
        let (file, fr) = found.remove(0);
        let src = fx.sources.get(&file).expect("file exists");
        let Some(range) = src.source_range_for_folded(fr) else {
            continue;
        };
        let (l0, _) = src.line_col(range.start);
        let (l1, _) = src.line_col(range.end);
        let quads = quads_for(&chars[start..end], truths.len() % 3);
        let nm = format!("t{}", truths.len());
        let bbox = union(&quads);
        let mut spec = AnnotationSpec::new(page_no, "Highlight", bbox);
        spec.nm = Some(nm.clone());
        spec.author = Some(["Prof. Hale", "Dr. Okafor"][truths.len() % 2].into());
        let mut truth_range = range.clone();
        match case {
            Case::Highlight => spec.quads = quads,
            Case::HighlightNote => {
                spec.quads = quads;
                spec.contents = Some("Please clarify this part.".into());
            }
            Case::Delete => {
                spec.subtype = "StrikeOut".into();
                spec.quads = quads;
            }
            Case::Replace => {
                spec.subtype = "StrikeOut".into();
                spec.quads = quads;
                spec.contents = Some("Replace with “sharper wording”".into());
            }
            Case::Question => {
                spec.subtype = "Underline".into();
                spec.quads = quads;
                spec.contents = Some("Is this really true?".into());
            }
            Case::Squiggly => {
                spec.subtype = "Squiggly".into();
                spec.quads = quads;
                // Some apps omit /Contents entirely.
            }
            Case::Insert => {
                let last = chars[end - 1];
                let x = last.bbox[2];
                spec.subtype = "Caret".into();
                spec.rect = [x - 3.0, last.bbox[1] - 2.0, x + 3.0, last.bbox[1] + 4.0];
                spec.contents = Some("and its variants".into());
                truth_range = range.end..range.end;
            }
            Case::StickyNote => {
                let last = chars[end - 1];
                spec.subtype = "Text".into();
                // Viewers put the icon's top-left corner where the reviewer clicked.
                let mid = f32::midpoint(last.bbox[1], last.bbox[3]) + 2.0;
                spec.rect = [last.bbox[2] + 2.0, mid - 18.0, last.bbox[2] + 20.0, mid];
                spec.contents = Some("Why is this needed?".into());
                spec.names.push(("Name".into(), "Comment".into()));
            }
        }
        used.push((page_no, start..end));
        specs.push(spec);
        truths.push(Truth {
            nm,
            case,
            file,
            range: truth_range,
            lines: l0..l1 + 1,
        });
    }
    let pdf = write_annotations(&fx.pdf, &specs).map_err(|e| e.to_string())?;
    Ok((pdf, truths))
}

fn trim(s: &str) -> String {
    s.trim().to_owned()
}

/// One quad per text line, in one of several point orders apps use.
fn quads_for(chars: &[proofback_pdf::TextChar], variant: usize) -> Vec<[f32; 8]> {
    let mut lines: Vec<[f32; 4]> = Vec::new();
    for c in chars.iter().filter(|c| !c.c.is_whitespace()) {
        match lines.last_mut() {
            Some(l) if c.bbox[1] < l[3] && c.bbox[3] > l[1] => {
                l[0] = l[0].min(c.bbox[0]);
                l[1] = l[1].min(c.bbox[1]);
                l[2] = l[2].max(c.bbox[2]);
                l[3] = l[3].max(c.bbox[3]);
            }
            _ => lines.push(c.bbox),
        }
    }
    let pad = if variant == 2 { 1.0 } else { 0.0 };
    lines
        .into_iter()
        .map(|[x0, y0, x1, y1]| {
            let (x0, y0, x1, y1) = (x0 - pad, y0 - pad, x1 + pad, y1 + pad);
            match variant {
                1 => [x0, y0, x1, y0, x1, y1, x0, y1],
                _ => [x0, y1, x1, y1, x0, y0, x1, y0],
            }
        })
        .collect()
}

fn union(quads: &[[f32; 8]]) -> [f32; 4] {
    let mut r = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for q in quads {
        for k in 0..4 {
            r[0] = r[0].min(q[2 * k]);
            r[1] = r[1].min(q[2 * k + 1]);
            r[2] = r[2].max(q[2 * k]);
            r[3] = r[3].max(q[2 * k + 1]);
        }
    }
    r
}

/// Counts for one evaluation run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    pub total: usize,
    /// Items whose case expects an exact answer.
    pub exact_expected: usize,
    pub exact_correct: usize,
    pub line_correct: usize,
    /// Confident answers (exact or line) that were wrong.
    pub wrong: usize,
    pub page_only: usize,
    pub missing: usize,
    pub by_case: HashMap<String, CaseMetrics>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CaseMetrics {
    pub total: usize,
    pub exact: usize,
    pub line: usize,
    pub wrong: usize,
    pub page: usize,
}

impl Metrics {
    pub fn exact_recall(&self) -> f64 {
        ratio(self.exact_correct, self.exact_expected)
    }

    pub fn line_rate(&self) -> f64 {
        ratio(self.line_correct, self.total)
    }

    pub fn wrong_rate(&self) -> f64 {
        ratio(self.wrong, self.total)
    }

    pub fn page_rate(&self) -> f64 {
        ratio(self.page_only + self.missing, self.total)
    }

    pub fn merge(&mut self, o: &Metrics) {
        self.total += o.total;
        self.exact_expected += o.exact_expected;
        self.exact_correct += o.exact_correct;
        self.line_correct += o.line_correct;
        self.wrong += o.wrong;
        self.page_only += o.page_only;
        self.missing += o.missing;
        for (k, v) in &o.by_case {
            let e = self.by_case.entry(k.clone()).or_default();
            e.total += v.total;
            e.exact += v.exact;
            e.line += v.line;
            e.wrong += v.wrong;
            e.page += v.page;
        }
    }
}

fn ratio(a: usize, b: usize) -> f64 {
    if b == 0 { 1.0 } else { a as f64 / b as f64 }
}

/// Run the real import on an annotated PDF and score it against the truth.
pub fn evaluate(
    fx: &Fixture,
    pdfium: &pdfium_render::prelude::Pdfium,
    pdf: &[u8],
    truths: &[Truth],
    use_synctex: bool,
) -> Result<(Metrics, Vec<Item>), String> {
    let extracted = extract(pdfium, pdf, Limits::default()).map_err(|e| e.to_string())?;
    let texts: HashMap<u32, PageText> = extracted.texts.into_iter().map(|t| (t.page, t)).collect();
    let input = Input {
        round: "corpus",
        annotations: &extracted.annotations,
        boxes: &extracted.boxes,
        texts: &texts,
        synctex: use_synctex.then_some(&fx.synctex),
        project_root: &fx.root,
        sources: &fx.sources,
        anonymize: false,
    };
    let items = import(&input);
    let by_nm: HashMap<&str, &Item> = items
        .iter()
        .filter_map(|i| match &i.origin {
            Origin::Pdf { nm: Some(nm), .. } => Some((nm.as_str(), i)),
            _ => None,
        })
        .collect();
    let mut m = Metrics::default();
    for t in truths {
        m.total += 1;
        if t.case.expects_exact() {
            m.exact_expected += 1;
        }
        let cm = m.by_case.entry(format!("{:?}", t.case)).or_default();
        cm.total += 1;
        let Some(item) = by_nm.get(t.nm.as_str()) else {
            m.missing += 1;
            cm.page += 1;
            continue;
        };
        let src = fx.sources.get(&t.file).expect("truth file exists");
        let line_ok = item.target.as_ref().is_some_and(|tg| {
            tg.file == t.file && tg.line >= t.lines.start && tg.line < t.lines.end
        });
        let exact_ok = item.target.as_ref().is_some_and(|tg| {
            tg.file == t.file && {
                let r = tg.range.as_range();
                r == t.range
                    || (!t.range.is_empty()
                        && src.text[r].trim() == src.text[t.range.clone()].trim())
            }
        });
        match item.anchor.confidence {
            Confidence::Exact if exact_ok => {
                m.exact_correct += 1;
                m.line_correct += 1;
                cm.exact += 1;
            }
            Confidence::Exact | Confidence::Line if !line_ok => {
                m.wrong += 1;
                cm.wrong += 1;
            }
            Confidence::Exact | Confidence::Line | Confidence::Region => {
                m.line_correct += usize::from(line_ok);
                cm.line += 1;
            }
            Confidence::Page => {
                m.page_only += 1;
                cm.page += 1;
            }
        }
    }
    Ok((m, items))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        assert_eq!(
            (0..5).map(|_| a.next_u64()).collect::<Vec<_>>(),
            (0..5).map(|_| b.next_u64()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn quad_orders() {
        let c = proofback_pdf::TextChar {
            c: 'a',
            bbox: [10.0, 20.0, 15.0, 30.0],
        };
        assert_eq!(
            quads_for(&[c], 0)[0],
            [10.0, 30.0, 15.0, 30.0, 10.0, 20.0, 15.0, 20.0]
        );
        assert_eq!(
            quads_for(&[c], 1)[0],
            [10.0, 20.0, 15.0, 20.0, 15.0, 30.0, 10.0, 30.0]
        );
    }
}
