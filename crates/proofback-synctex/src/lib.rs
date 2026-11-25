//! Streaming parser and index for `SyncTeX` files (`.synctex` and `.synctex.gz`).
//!
//! `SyncTeX` records where every box, glue, kern and math node of a typeset page came
//! from in the TeX sources. proofback uses it for *coarse* location: a point on a PDF
//! page becomes a ranked list of `(file, line)` candidates, which the text aligner
//! then refines to a character range.
//!
//! The parser is deliberately strict about resource use (see [`Limits`]) because the
//! files may come from other people.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Component, Path, PathBuf};

use flate2::read::MultiGzDecoder;

/// TeX scaled points per TeX point.
const SP_PER_PT: f64 = 65536.0;
/// PDF big points per TeX point.
const BP_PER_PT: f64 = 72.0 / 72.27;

/// Errors produced while reading a `SyncTeX` file.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error while reading SyncTeX data: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a SyncTeX file (missing \"SyncTeX Version\" header)")]
    NotSyncTex,
    #[error("malformed SyncTeX record on line {line}: {reason}")]
    Malformed { line: usize, reason: &'static str },
    #[error("SyncTeX data exceeds the {what} limit ({limit})")]
    LimitExceeded { what: &'static str, limit: u64 },
}

/// Resource limits applied while parsing untrusted input.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Maximum decompressed size in bytes.
    pub max_bytes: u64,
    /// Maximum length of a single line in bytes.
    pub max_line: usize,
    /// Maximum number of node records across all pages.
    pub max_records: usize,
    /// Maximum number of pages.
    pub max_pages: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 1 << 30,
            max_line: 64 * 1024,
            max_records: 50_000_000,
            max_pages: 20_000,
        }
    }
}

/// The kind of a `SyncTeX` node record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `[` vertical box.
    VBox,
    /// `(` horizontal box (usually a typeset line).
    HBox,
    /// `v` void vertical box.
    VoidVBox,
    /// `h` void horizontal box.
    VoidHBox,
    /// `x` current point.
    Current,
    /// `k` kern.
    Kern,
    /// `g` glue.
    Glue,
    /// `$` math node.
    Math,
    /// `r` rule.
    Rule,
}

impl Kind {
    fn is_box(self) -> bool {
        matches!(
            self,
            Kind::VBox | Kind::HBox | Kind::VoidVBox | Kind::VoidHBox
        )
    }
}

/// One node record, with coordinates converted to PDF points measured from the
/// top-left corner of the page (y grows downwards, as in TeX).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Record {
    pub kind: Kind,
    /// Input tag (see [`SyncTex::input`]).
    pub tag: u32,
    /// 1-based source line.
    pub line: u32,
    /// Optional 1-based column (rarely emitted by engines).
    pub column: Option<u32>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub depth: f32,
    /// Index of the enclosing box within the page, if any.
    pub parent: Option<u32>,
}

impl Record {
    /// Bounding box `(left, top, right, bottom)` for boxes; a zero-height marker for
    /// other nodes.
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let (w, h, d) = (
            self.width.max(0.0),
            self.height.max(0.0),
            self.depth.max(0.0),
        );
        (self.x, self.y - h, self.x + w, self.y + d)
    }
}

/// All records of one output page.
#[derive(Debug, Clone, Default)]
pub struct Page {
    /// 1-based page number.
    pub number: u32,
    pub records: Vec<Record>,
}

/// A parsed `SyncTeX` file.
#[derive(Debug, Clone, Default)]
pub struct SyncTex {
    inputs: HashMap<u32, String>,
    pages: Vec<Page>,
}

/// A source location candidate returned by [`SyncTex::inverse`].
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub tag: u32,
    pub line: u32,
    /// Higher is better; the best candidate has score 1.0.
    pub score: f32,
}

/// A box on a page produced by a given source line, returned by [`SyncTex::forward`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub page: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub depth: f32,
}

impl SyncTex {
    /// Parse `SyncTeX` data, transparently decompressing gzip input.
    pub fn parse<R: Read>(reader: R, limits: Limits) -> Result<Self, Error> {
        let mut buffered = BufReader::new(reader);
        let is_gzip = {
            let head = buffered.fill_buf()?;
            head.len() >= 2 && head[0] == 0x1f && head[1] == 0x8b
        };
        if is_gzip {
            Parser::new(limits).run(BufReader::new(MultiGzDecoder::new(buffered)))
        } else {
            Parser::new(limits).run(buffered)
        }
    }

    /// Parse a file from disk.
    pub fn open(path: impl AsRef<Path>, limits: Limits) -> Result<Self, Error> {
        Self::parse(std::fs::File::open(path)?, limits)
    }

    /// Path recorded for an input tag, exactly as the engine wrote it.
    pub fn input(&self, tag: u32) -> Option<&str> {
        self.inputs.get(&tag).map(String::as_str)
    }

    /// All `(tag, path)` pairs, sorted by tag.
    pub fn inputs(&self) -> Vec<(u32, &str)> {
        let mut v: Vec<_> = self.inputs.iter().map(|(t, p)| (*t, p.as_str())).collect();
        v.sort_unstable_by_key(|(t, _)| *t);
        v
    }

    /// Find the tag whose recorded path resolves to `rel` inside `project_root`.
    pub fn tag_for(&self, project_root: &Path, rel: &Path) -> Option<u32> {
        self.inputs().into_iter().find_map(|(tag, p)| {
            (resolve_input(project_root, p).as_deref() == Some(rel)).then_some(tag)
        })
    }

    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    /// Page by 1-based number.
    pub fn page(&self, number: u32) -> Option<&Page> {
        self.pages.iter().find(|p| p.number == number)
    }

    /// Total number of node records.
    pub fn record_count(&self) -> usize {
        self.pages.iter().map(|p| p.records.len()).sum()
    }

    /// Inverse search: source candidates for a point on a page, in PDF points from
    /// the top-left corner. Returns candidates sorted by descending score.
    pub fn inverse(&self, page: u32, x: f32, y: f32) -> Vec<Candidate> {
        let Some(page) = self.page(page) else {
            return Vec::new();
        };
        inverse_on_page(page, x, y)
    }

    /// Forward search: every box or node produced by `(tag, line)`.
    pub fn forward(&self, tag: u32, line: u32) -> Vec<Hit> {
        let mut hits = Vec::new();
        for page in &self.pages {
            for r in &page.records {
                if r.tag == tag && r.line == line {
                    hits.push(Hit {
                        page: page.number,
                        x: r.x,
                        y: r.y,
                        width: r.width,
                        height: r.height,
                        depth: r.depth,
                    });
                }
            }
        }
        hits
    }
}

fn inverse_on_page(page: &Page, px: f32, py: f32) -> Vec<Candidate> {
    const TOL: f32 = 1.0;
    let recs = &page.records;
    // 1. The smallest horizontal box (a typeset line) that contains the point.
    let mut best_line: Option<(usize, f32)> = None;
    let mut nearest_line: Option<(usize, f32)> = None;
    for (i, r) in recs.iter().enumerate() {
        if r.kind != Kind::HBox || r.width <= 0.0 {
            continue;
        }
        let (l, t, rr, b) = r.bounds();
        let inside_y = py >= t - TOL && py <= b + TOL;
        let inside_x = px >= l - TOL && px <= rr + TOL;
        if inside_x && inside_y {
            let area = (rr - l) * (b - t).max(0.1);
            if best_line.is_none_or(|(_, a)| area < a) {
                best_line = Some((i, area));
            }
        } else if inside_x {
            let dist = if py < t { t - py } else { py - b };
            if nearest_line.is_none_or(|(_, d)| dist < d) {
                nearest_line = Some((i, dist));
            }
        }
    }
    let (line_idx, base_score) = match (best_line, nearest_line) {
        (Some((i, _)), _) => (i, 1.0),
        (None, Some((i, d))) => (i, (1.0 / (1.0 + d / 12.0)).min(0.8)),
        (None, None) => return fallback_nearest(recs, px, py),
    };
    let line_box = &recs[line_idx];

    // 2. Leaf nodes inside that line, ordered by x. The last one starting at or
    // before the point names the source line of the text under the point.
    let mut leaves: Vec<&Record> = recs
        .iter()
        .filter(|r| !r.kind.is_box() && r.parent == Some(line_idx as u32))
        .collect();
    leaves.sort_by(|a, b| a.x.total_cmp(&b.x));

    let mut out: Vec<Candidate> = Vec::new();
    let mut push = |tag: u32, line: u32, score: f32| {
        if let Some(c) = out.iter_mut().find(|c| c.tag == tag && c.line == line) {
            c.score = c.score.max(score);
        } else {
            out.push(Candidate { tag, line, score });
        }
    };
    if let Some(pos) = leaves.iter().rposition(|r| r.x <= px + TOL) {
        let r = leaves[pos];
        push(r.tag, r.line, base_score);
        if let Some(next) = leaves.get(pos + 1) {
            push(next.tag, next.line, base_score * 0.6);
        }
        if pos > 0 {
            let prev = leaves[pos - 1];
            push(prev.tag, prev.line, base_score * 0.5);
        }
    } else if let Some(first) = leaves.first() {
        push(first.tag, first.line, base_score * 0.9);
    }
    push(line_box.tag, line_box.line, base_score * 0.4);
    // Other lines contributing to the same typeset line (a paragraph usually spans
    // several source lines) get a small score so the aligner can consider them.
    for r in &leaves {
        push(r.tag, r.line, base_score * 0.2);
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.line.cmp(&b.line)));
    out
}

fn fallback_nearest(recs: &[Record], px: f32, py: f32) -> Vec<Candidate> {
    let best = recs.iter().filter(|r| !r.kind.is_box()).min_by(|a, b| {
        let da = (a.x - px).abs() + 4.0 * (a.y - py).abs();
        let db = (b.x - px).abs() + 4.0 * (b.y - py).abs();
        da.total_cmp(&db)
    });
    best.map(|r| {
        vec![Candidate {
            tag: r.tag,
            line: r.line,
            score: 0.2,
        }]
    })
    .unwrap_or_default()
}

/// Resolve a path recorded by the engine to a path relative to `project_root`.
///
/// Engines record absolute paths, `dir/./file` forms, or paths relative to the
/// build directory, possibly with Windows separators. The result is `None` when the
/// path cannot be placed inside the project (system packages, or anything that would
/// escape the root), which is also the safety check against path traversal.
pub fn resolve_input(project_root: &Path, recorded: &str) -> Option<PathBuf> {
    let recorded = recorded.replace('\\', "/");
    let candidates: Vec<String> = {
        let mut v = Vec::new();
        let root = project_root.to_string_lossy().replace('\\', "/");
        let root = root.trim_end_matches('/');
        if !root.is_empty()
            && let Some(rest) = recorded.strip_prefix(&format!("{root}/"))
        {
            v.push(rest.to_owned());
        }
        if let Some(idx) = recorded.rfind("/./") {
            v.push(recorded[idx + 3..].to_owned());
        }
        if !recorded.starts_with('/') && !has_drive_prefix(&recorded) {
            v.push(recorded.clone());
        }
        v
    };
    candidates.into_iter().find_map(|c| normalize_relative(&c))
}

fn has_drive_prefix(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Lexically normalise a relative path, rejecting anything that leaves the root.
fn normalize_relative(p: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for comp in Path::new(p).components() {
        match comp {
            Component::Normal(c) => out.push(c),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

struct Parser {
    limits: Limits,
    inputs: HashMap<u32, String>,
    pages: Vec<Page>,
    magnification: f64,
    unit: f64,
    x_offset: f64,
    y_offset: f64,
    records: usize,
}

impl Parser {
    fn new(limits: Limits) -> Self {
        Self {
            limits,
            inputs: HashMap::new(),
            pages: Vec::new(),
            magnification: 1000.0,
            unit: 1.0,
            x_offset: 0.0,
            y_offset: 0.0,
            records: 0,
        }
    }

    fn to_bp(&self, v: i64) -> f32 {
        (v as f64 * self.unit * (self.magnification / 1000.0) / SP_PER_PT * BP_PER_PT) as f32
    }

    fn run<R: BufRead>(mut self, mut reader: R) -> Result<SyncTex, Error> {
        let mut buf = Vec::with_capacity(256);
        let mut total: u64 = 0;
        let mut line_no = 0usize;
        let mut seen_header = false;
        let mut current: Option<Page> = None;
        let mut stack: Vec<u32> = Vec::new();

        loop {
            buf.clear();
            let n = read_line_limited(&mut reader, &mut buf, self.limits.max_line)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > self.limits.max_bytes {
                return Err(Error::LimitExceeded {
                    what: "decompressed size",
                    limit: self.limits.max_bytes,
                });
            }
            line_no += 1;
            while matches!(buf.last(), Some(b'\n' | b'\r')) {
                buf.pop();
            }
            let line = String::from_utf8_lossy(&buf);
            let line = line.as_ref();

            if !seen_header {
                if line.starts_with("SyncTeX Version:") {
                    seen_header = true;
                    continue;
                }
                return Err(Error::NotSyncTex);
            }
            if line.is_empty() {
                continue;
            }

            // Preamble and postamble key/value lines, plus inputs that may appear
            // anywhere in the content.
            if let Some(rest) = line.strip_prefix("Input:") {
                let (tag, path) = rest.split_once(':').ok_or(Error::Malformed {
                    line: line_no,
                    reason: "Input record",
                })?;
                let tag: u32 = tag.parse().map_err(|_| Error::Malformed {
                    line: line_no,
                    reason: "Input tag",
                })?;
                self.inputs.insert(tag, path.to_owned());
                continue;
            }
            if let Some(v) = line.strip_prefix("Magnification:") {
                self.magnification = v.trim().parse().unwrap_or(1000.0);
                continue;
            }
            if let Some(v) = line.strip_prefix("Unit:") {
                self.unit = v.trim().parse().unwrap_or(1.0);
                continue;
            }
            if let Some(v) = line.strip_prefix("X Offset:") {
                self.x_offset = v.trim().parse().unwrap_or(0.0);
                continue;
            }
            if let Some(v) = line.strip_prefix("Y Offset:") {
                self.y_offset = v.trim().parse().unwrap_or(0.0);
                continue;
            }

            let first = line.as_bytes()[0];
            match first {
                b'{' => {
                    let number: u32 = line[1..].parse().map_err(|_| Error::Malformed {
                        line: line_no,
                        reason: "page start",
                    })?;
                    if self.pages.len() >= self.limits.max_pages {
                        return Err(Error::LimitExceeded {
                            what: "page count",
                            limit: self.limits.max_pages as u64,
                        });
                    }
                    current = Some(Page {
                        number,
                        records: Vec::new(),
                    });
                    stack.clear();
                }
                b'}' => {
                    if let Some(p) = current.take() {
                        self.pages.push(p);
                    }
                    stack.clear();
                }
                b']' | b')' => {
                    stack.pop();
                }
                b'[' | b'(' | b'v' | b'h' | b'x' | b'k' | b'g' | b'$' | b'r' => {
                    let Some(page) = current.as_mut() else {
                        continue;
                    };
                    let kind = match first {
                        b'[' => Kind::VBox,
                        b'(' => Kind::HBox,
                        b'v' => Kind::VoidVBox,
                        b'h' => Kind::VoidHBox,
                        b'x' => Kind::Current,
                        b'k' => Kind::Kern,
                        b'g' => Kind::Glue,
                        b'$' => Kind::Math,
                        _ => Kind::Rule,
                    };
                    let Some(fields) = parse_node(&line[1..]) else {
                        return Err(Error::Malformed {
                            line: line_no,
                            reason: "node record",
                        });
                    };
                    self.records += 1;
                    if self.records > self.limits.max_records {
                        return Err(Error::LimitExceeded {
                            what: "record count",
                            limit: self.limits.max_records as u64,
                        });
                    }
                    let x_off = self.x_offset;
                    let y_off = self.y_offset;
                    let rec = Record {
                        kind,
                        tag: fields.tag,
                        line: fields.line,
                        column: fields.column,
                        // Offsets come from the file: saturate rather than overflow.
                        x: self.to_bp(fields.x.saturating_add(x_off as i64)),
                        y: self.to_bp(fields.y.saturating_add(y_off as i64)),
                        width: self.to_bp(fields.w),
                        height: self.to_bp(fields.h),
                        depth: self.to_bp(fields.d),
                        parent: stack.last().copied(),
                    };
                    let idx = page.records.len() as u32;
                    page.records.push(rec);
                    if matches!(kind, Kind::VBox | Kind::HBox) {
                        stack.push(idx);
                    }
                }
                // Byte counts, form references and anything newer we do not need.
                _ => {}
            }
        }
        if !seen_header {
            return Err(Error::NotSyncTex);
        }
        if let Some(p) = current.take() {
            self.pages.push(p);
        }
        Ok(SyncTex {
            inputs: self.inputs,
            pages: self.pages,
        })
    }
}

struct NodeFields {
    tag: u32,
    line: u32,
    column: Option<u32>,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    d: i64,
}

/// Parse `tag,line[,column]:x,y[:W[,H,D]]`.
fn parse_node(s: &str) -> Option<NodeFields> {
    let mut parts = s.split(':');
    let link = parts.next()?;
    let pos = parts.next()?;
    let size = parts.next();
    let mut l = link.split(',');
    let tag = l.next()?.parse().ok()?;
    let line = l.next()?.parse().ok()?;
    let column = l.next().and_then(|c| c.parse().ok());
    let (x, y) = pos.split_once(',')?;
    let (x, y) = (x.parse().ok()?, y.parse().ok()?);
    let (mut w, mut h, mut d) = (0, 0, 0);
    if let Some(size) = size {
        let mut it = size.split(',');
        w = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        h = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        d = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    }
    Some(NodeFields {
        tag,
        line,
        column,
        x,
        y,
        w,
        h,
        d,
    })
}

/// Read one line, failing if it is longer than `max` bytes.
fn read_line_limited<R: BufRead>(r: &mut R, buf: &mut Vec<u8>, max: usize) -> Result<usize, Error> {
    let mut read = 0;
    loop {
        let available = r.fill_buf()?;
        if available.is_empty() {
            return Ok(read);
        }
        let (chunk, done) = match available.iter().position(|&b| b == b'\n') {
            Some(i) => (&available[..=i], true),
            None => (available, false),
        };
        if buf.len() + chunk.len() > max {
            return Err(Error::LimitExceeded {
                what: "line length",
                limit: max as u64,
            });
        }
        buf.extend_from_slice(chunk);
        let n = chunk.len();
        r.consume(n);
        read += n;
        if done {
            return Ok(read);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "SyncTeX Version:1\nInput:1:/work/thesis/./main.tex\nInput:2:/usr/share/texmf/report.cls\nInput:3:/work/thesis/./chapters/a.tex\nOutput:pdf\nMagnification:1000\nUnit:1\nX Offset:0\nY Offset:0\nContent:\n!100\n{1\n[1,5:4736286,45984660:27131904,41248374,0\n(3,4:4736286,10000000:27131904,500000,100000\nx3,4:4736286,10000000\ng3,4:9000000,10000000\nx3,5:15000000,10000000\nk3,5:20000000,10000000:100\n)\n(3,6:4736286,12000000:27131904,500000,100000\nx3,6:4736286,12000000\n$3,7:10000000,12000000\n)\n]\n}1\n!20\nPostamble:\nCount:9\n";

    fn sample() -> SyncTex {
        SyncTex::parse(SAMPLE.as_bytes(), Limits::default()).unwrap()
    }

    #[test]
    fn parses_inputs_and_pages() {
        let s = sample();
        assert_eq!(s.inputs().len(), 3);
        assert_eq!(s.pages().len(), 1);
        assert_eq!(s.record_count(), 9);
        let p = s.page(1).unwrap();
        assert_eq!(p.records[1].kind, Kind::HBox);
        assert_eq!(p.records[2].parent, Some(1));
    }

    #[test]
    fn converts_scaled_points_to_pdf_points() {
        let s = sample();
        let r = &s.page(1).unwrap().records[0];
        // 4736286 sp is one inch: 72 PDF points.
        assert!((r.x - 72.0).abs() < 0.01, "{}", r.x);
    }

    #[test]
    fn inverse_picks_the_source_line_under_the_point() {
        let s = sample();
        let to_bp = |sp: f64| (sp / 65536.0 * 72.0 / 72.27) as f32;
        let c = s.inverse(1, to_bp(16_000_000.0), to_bp(9_950_000.0));
        assert_eq!((c[0].tag, c[0].line), (3, 5));
        let c = s.inverse(1, to_bp(6_000_000.0), to_bp(9_950_000.0));
        assert_eq!((c[0].tag, c[0].line), (3, 4));
        let c = s.inverse(1, to_bp(11_000_000.0), to_bp(11_950_000.0));
        assert_eq!((c[0].tag, c[0].line), (3, 7));
    }

    #[test]
    fn forward_finds_records() {
        let s = sample();
        assert_eq!(s.forward(3, 5).len(), 2);
        assert_eq!(s.forward(3, 99).len(), 0);
    }

    #[test]
    fn gzip_input_is_detected() {
        use std::io::Write;
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        enc.write_all(SAMPLE.as_bytes()).unwrap();
        let gz = enc.finish().unwrap();
        let s = SyncTex::parse(gz.as_slice(), Limits::default()).unwrap();
        assert_eq!(s.record_count(), 9);
    }

    #[test]
    fn rejects_non_synctex() {
        assert!(matches!(
            SyncTex::parse(&b"hello\n"[..], Limits::default()),
            Err(Error::NotSyncTex)
        ));
        assert!(matches!(
            SyncTex::parse(&b""[..], Limits::default()),
            Err(Error::NotSyncTex)
        ));
    }

    #[test]
    fn enforces_limits() {
        let limits = Limits {
            max_records: 3,
            ..Limits::default()
        };
        assert!(matches!(
            SyncTex::parse(SAMPLE.as_bytes(), limits),
            Err(Error::LimitExceeded { .. })
        ));
        let limits = Limits {
            max_line: 10,
            ..Limits::default()
        };
        assert!(matches!(
            SyncTex::parse(SAMPLE.as_bytes(), limits),
            Err(Error::LimitExceeded { .. })
        ));
    }

    #[test]
    fn resolves_inputs_inside_the_project_only() {
        let root = Path::new("/work/thesis");
        assert_eq!(
            resolve_input(root, "/work/thesis/./chapters/a.tex"),
            Some(PathBuf::from("chapters/a.tex"))
        );
        assert_eq!(
            resolve_input(root, "/other/place/./main.tex"),
            Some(PathBuf::from("main.tex"))
        );
        assert_eq!(
            resolve_input(root, "C:\\Users\\x\\thesis\\.\\ch\\b.tex"),
            Some(PathBuf::from("ch/b.tex"))
        );
        assert_eq!(resolve_input(root, "/usr/share/texmf/report.cls"), None);
        assert_eq!(resolve_input(root, "../outside.tex"), None);
        assert_eq!(resolve_input(root, "/work/thesis/../etc/passwd"), None);
        assert_eq!(resolve_input(root, "./a/../../x.tex"), None);
    }

    /// Found by the fuzzer: a huge `Y Offset` overflowed when added to a coordinate.
    #[test]
    fn huge_offsets_do_not_overflow() {
        let text = "SyncTeX Version:1\nInput:1:/w/./main.tex\nOutput:pdf\nMagnification:1000\nUnit:1\n\
                    X Offset:-9223372036854775807\nY Offset:4306466846268853802\nContent:\n!1\n{1\n\
                    [1,5:9223372036854775000,9223372036854775000:10,10,0\n]\n}1\nPostamble:\n";
        let s = SyncTex::parse(text.as_bytes(), Limits::default()).expect("parses");
        let r = &s.pages()[0].records[0];
        assert!(r.x.is_finite() && r.y.is_finite());
    }
}
