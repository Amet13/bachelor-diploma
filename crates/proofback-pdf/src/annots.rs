//! Annotation dictionaries, read and written with `lopdf`.

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat, dictionary};

use crate::Error;

/// Limits applied to untrusted PDFs.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_bytes: u64,
    pub max_pages: usize,
    pub max_annotations: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 512 << 20,
            max_pages: 20_000,
            max_annotations: 100_000,
        }
    }
}

/// A page's media box `[x0, y0, x1, y1]`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PageBox {
    /// 1-based page number.
    pub page: u32,
    pub media: [f32; 4],
}

impl PageBox {
    pub fn width(&self) -> f32 {
        self.media[2] - self.media[0]
    }

    pub fn height(&self) -> f32 {
        self.media[3] - self.media[1]
    }

    /// Convert a PDF point to top-left page coordinates (as used by SyncTeX).
    pub fn to_top_left(&self, x: f32, y: f32) -> (f32, f32) {
        (x - self.media[0], self.media[3] - y)
    }

    /// Convert top-left page coordinates back to PDF space.
    pub fn from_top_left(&self, x: f32, y: f32) -> (f32, f32) {
        (x + self.media[0], self.media[3] - y)
    }
}

/// An annotation as found in the PDF.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PdfAnnotation {
    /// 1-based page number.
    pub page: u32,
    /// Position in the page's `/Annots` array.
    pub index: usize,
    /// `/Subtype`, e.g. `Highlight`.
    pub subtype: String,
    /// `/NM` unique name.
    pub nm: Option<String>,
    /// `/T` author.
    pub author: Option<String>,
    /// `/Contents` (falls back to the plain text of `/RC`).
    pub contents: String,
    /// `/CreationDate` or `/M`, converted to RFC 3339 when possible.
    pub created: Option<String>,
    /// `/Rect` as `[x0, y0, x1, y1]`.
    pub rect: [f32; 4],
    /// `/QuadPoints`, 8 numbers per quad.
    pub quads: Vec<[f32; 8]>,
    /// `/C` colour as `#rrggbb`.
    pub color: Option<String>,
    /// Index (into the returned list) of the annotation this one replies to.
    pub in_reply_to: Option<usize>,
    /// `/IT` intent, e.g. `FreeTextTypeWriter`.
    pub intent: Option<String>,
}

/// Subtypes that never carry reviewer feedback.
const IGNORED: &[&str] = &[
    "Popup",
    "Link",
    "Widget",
    "Screen",
    "PrinterMark",
    "TrapNet",
    "Watermark",
    "3D",
    "RichMedia",
];

/// Read all feedback annotations and page boxes from a PDF.
pub fn read_annotations(
    bytes: &[u8],
    limits: Limits,
) -> Result<(Vec<PageBox>, Vec<PdfAnnotation>), Error> {
    if bytes.len() as u64 > limits.max_bytes {
        return Err(Error::LimitExceeded {
            what: "file size",
            limit: limits.max_bytes,
        });
    }
    let doc = load(bytes)?;
    let pages = doc.get_pages();
    if pages.len() > limits.max_pages {
        return Err(Error::LimitExceeded {
            what: "page count",
            limit: limits.max_pages as u64,
        });
    }
    let mut boxes = Vec::with_capacity(pages.len());
    let mut out: Vec<PdfAnnotation> = Vec::new();
    let mut by_object: HashMap<ObjectId, usize> = HashMap::new();
    let mut pending_irt: Vec<(usize, ObjectId)> = Vec::new();

    for (&number, &page_id) in &pages {
        let page = doc
            .get_dictionary(page_id)
            .map_err(|e| Error::Parse(e.to_string()))?;
        let media = inherited(&doc, page_id, b"MediaBox")
            .and_then(|o| rect_of(&doc, o))
            .unwrap_or([0.0, 0.0, 612.0, 792.0]);
        boxes.push(PageBox {
            page: number,
            media,
        });
        let Some(annots) = page
            .get(b"Annots")
            .ok()
            .and_then(|o| resolve(&doc, o).as_array().ok())
        else {
            continue;
        };
        for (index, entry) in annots.iter().enumerate() {
            let Ok(id) = entry.as_reference() else {
                continue;
            };
            let Ok(dict) = doc.get_dictionary(id) else {
                continue;
            };
            let subtype = name(&doc, dict, b"Subtype").unwrap_or_default();
            if subtype.is_empty() || IGNORED.contains(&subtype.as_str()) {
                continue;
            }
            if out.len() >= limits.max_annotations {
                return Err(Error::LimitExceeded {
                    what: "annotation count",
                    limit: limits.max_annotations as u64,
                });
            }
            let contents = text(&doc, dict, b"Contents")
                .filter(|s| !s.trim().is_empty())
                .or_else(|| text(&doc, dict, b"RC").map(|rc| strip_xml(&rc)))
                .unwrap_or_default();
            let quads = dict
                .get(b"QuadPoints")
                .ok()
                .and_then(|o| resolve(&doc, o).as_array().ok())
                .map(|a| {
                    let nums: Vec<f32> = a.iter().filter_map(|o| number_of(&doc, o)).collect();
                    nums.chunks_exact(8)
                        .map(|c| [c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]])
                        .collect()
                })
                .unwrap_or_default();
            let color = dict
                .get(b"C")
                .ok()
                .and_then(|o| resolve(&doc, o).as_array().ok())
                .and_then(|a| {
                    let n: Vec<f32> = a.iter().filter_map(|o| number_of(&doc, o)).collect();
                    match n.as_slice() {
                        [r, g, b] => Some(hex(*r, *g, *b)),
                        [gray] => Some(hex(*gray, *gray, *gray)),
                        [c, m, y, k] => Some(hex(
                            (1.0 - c) * (1.0 - k),
                            (1.0 - m) * (1.0 - k),
                            (1.0 - y) * (1.0 - k),
                        )),
                        _ => None,
                    }
                });
            let idx = out.len();
            by_object.insert(id, idx);
            if let Ok(irt) = dict.get(b"IRT").and_then(Object::as_reference) {
                pending_irt.push((idx, irt));
            }
            out.push(PdfAnnotation {
                page: number,
                index,
                subtype,
                nm: text(&doc, dict, b"NM"),
                author: text(&doc, dict, b"T"),
                contents,
                created: text(&doc, dict, b"CreationDate")
                    .or_else(|| text(&doc, dict, b"M"))
                    .map(|d| pdf_date(&d)),
                rect: dict
                    .get(b"Rect")
                    .ok()
                    .and_then(|o| rect_of(&doc, o))
                    .unwrap_or([0.0; 4]),
                quads,
                color,
                in_reply_to: None,
                intent: name(&doc, dict, b"IT"),
            });
        }
    }
    for (idx, target) in pending_irt {
        out[idx].in_reply_to = by_object.get(&target).copied();
    }
    Ok((boxes, out))
}

/// A new annotation to add to a PDF.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotationSpec {
    pub page: u32,
    pub subtype: String,
    pub rect: [f32; 4],
    pub quads: Vec<[f32; 8]>,
    pub contents: Option<String>,
    pub author: Option<String>,
    pub nm: Option<String>,
    pub color: Option<[f32; 3]>,
    /// Reply to the annotation at this `(page, index in /Annots)`.
    pub in_reply_to: Option<(u32, usize)>,
    /// Extra name entries such as `/Name /Comment` or `/StateModel /Review`.
    pub names: Vec<(String, String)>,
    /// Extra text entries such as `/State`.
    pub texts: Vec<(String, String)>,
}

impl AnnotationSpec {
    pub fn new(page: u32, subtype: &str, rect: [f32; 4]) -> Self {
        Self {
            page,
            subtype: subtype.to_owned(),
            rect,
            quads: Vec::new(),
            contents: None,
            author: None,
            nm: None,
            color: None,
            in_reply_to: None,
            names: Vec::new(),
            texts: Vec::new(),
        }
    }
}

/// Return a copy of the PDF with the given annotations appended to their pages.
/// The input is never modified.
pub fn write_annotations(bytes: &[u8], specs: &[AnnotationSpec]) -> Result<Vec<u8>, Error> {
    let mut doc = load(bytes)?;
    let pages = doc.get_pages();
    for spec in specs {
        let page_id = *pages
            .get(&spec.page)
            .ok_or_else(|| Error::Write(format!("no page {}", spec.page)))?;
        let mut d = dictionary! {
            "Type" => "Annot",
            "Subtype" => Object::Name(spec.subtype.clone().into_bytes()),
            "Rect" => spec.rect.iter().map(|&v| Object::Real(v)).collect::<Vec<_>>(),
            "F" => 4,
            "P" => Object::Reference(page_id),
        };
        if !spec.quads.is_empty() {
            let q: Vec<Object> = spec
                .quads
                .iter()
                .flatten()
                .map(|&v| Object::Real(v))
                .collect();
            d.set("QuadPoints", q);
        }
        if let Some(c) = &spec.contents {
            d.set("Contents", pdf_text(c));
        }
        if let Some(a) = &spec.author {
            d.set("T", pdf_text(a));
        }
        if let Some(nm) = &spec.nm {
            d.set("NM", pdf_text(nm));
        }
        if let Some([r, g, b]) = spec.color {
            d.set("C", vec![Object::Real(r), Object::Real(g), Object::Real(b)]);
        }
        for (k, v) in &spec.names {
            d.set(k.as_bytes().to_vec(), Object::Name(v.clone().into_bytes()));
        }
        for (k, v) in &spec.texts {
            d.set(k.as_bytes().to_vec(), pdf_text(v));
        }
        if let Some((p, i)) = spec.in_reply_to {
            let target = annotation_ref(&doc, &pages, p, i)
                .ok_or_else(|| Error::Write(format!("no annotation {i} on page {p}")))?;
            d.set("IRT", Object::Reference(target));
        }
        let id = doc.add_object(Object::Dictionary(d));
        append_annot(&mut doc, page_id, id)?;
    }
    save(&mut doc)
}

/// Remove document metadata that could identify the author or their machine
/// (`/Info` and XMP metadata).
pub fn strip_metadata(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let mut doc = load(bytes)?;
    doc.trailer.remove(b"Info");
    if let Ok(root_id) = doc.trailer.get(b"Root").and_then(Object::as_reference) {
        if let Ok(root) = doc.get_dictionary_mut(root_id) {
            root.remove(b"Metadata");
        }
    }
    save(&mut doc)
}

fn load(bytes: &[u8]) -> Result<Document, Error> {
    let doc = Document::load_mem(bytes).map_err(|e| Error::Parse(e.to_string()))?;
    if doc.is_encrypted() {
        return Err(Error::Encrypted);
    }
    Ok(doc)
}

fn save(doc: &mut Document) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| Error::Write(e.to_string()))?;
    Ok(out)
}

fn annotation_ref(
    doc: &Document,
    pages: &std::collections::BTreeMap<u32, ObjectId>,
    page: u32,
    index: usize,
) -> Option<ObjectId> {
    let page = doc.get_dictionary(*pages.get(&page)?).ok()?;
    let annots = resolve(doc, page.get(b"Annots").ok()?).as_array().ok()?;
    annots.get(index)?.as_reference().ok()
}

fn append_annot(doc: &mut Document, page_id: ObjectId, annot: ObjectId) -> Result<(), Error> {
    let existing = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|p| p.get(b"Annots").ok().cloned());
    match existing {
        Some(Object::Reference(arr_id)) => {
            let arr = doc
                .get_object_mut(arr_id)
                .map_err(|e| Error::Write(e.to_string()))?;
            arr.as_array_mut()
                .map_err(|e| Error::Write(e.to_string()))?
                .push(Object::Reference(annot));
        }
        Some(Object::Array(mut a)) => {
            a.push(Object::Reference(annot));
            doc.get_dictionary_mut(page_id)
                .map_err(|e| Error::Write(e.to_string()))?
                .set("Annots", a);
        }
        _ => {
            doc.get_dictionary_mut(page_id)
                .map_err(|e| Error::Write(e.to_string()))?
                .set("Annots", vec![Object::Reference(annot)]);
        }
    }
    Ok(())
}

fn resolve<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    let mut cur = o;
    for _ in 0..16 {
        match cur {
            Object::Reference(id) => match doc.get_object(*id) {
                Ok(next) => cur = next,
                Err(_) => return cur,
            },
            _ => return cur,
        }
    }
    cur
}

fn inherited<'a>(doc: &'a Document, page_id: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut id = page_id;
    for _ in 0..32 {
        let d = doc.get_dictionary(id).ok()?;
        if let Ok(v) = d.get(key) {
            return Some(v);
        }
        id = d.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

fn number_of(doc: &Document, o: &Object) -> Option<f32> {
    match resolve(doc, o) {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

fn rect_of(doc: &Document, o: &Object) -> Option<[f32; 4]> {
    let a = resolve(doc, o).as_array().ok()?;
    let n: Vec<f32> = a.iter().filter_map(|o| number_of(doc, o)).collect();
    if n.len() != 4 {
        return None;
    }
    Some([
        n[0].min(n[2]),
        n[1].min(n[3]),
        n[0].max(n[2]),
        n[1].max(n[3]),
    ])
}

fn name(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<String> {
    match resolve(doc, d.get(key).ok()?) {
        Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
        _ => None,
    }
}

fn text(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<String> {
    match resolve(doc, d.get(key).ok()?) {
        Object::String(bytes, _) => Some(decode_text_string(bytes)),
        Object::Stream(s) => {
            let data = s
                .decompressed_content()
                .unwrap_or_else(|_| s.content.clone());
            Some(String::from_utf8_lossy(&data).into_owned())
        }
        _ => None,
    }
}

/// Decode a PDF text string (UTF-16BE with BOM, UTF-8 with BOM, or PDFDocEncoding).
pub fn decode_text_string(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    bytes.iter().map(|&b| pdfdoc_char(b)).collect()
}

fn pdfdoc_char(b: u8) -> char {
    match b {
        0x80 => '•',
        0x81 => '†',
        0x82 => '‡',
        0x83 => '…',
        0x84 => '—',
        0x85 => '–',
        0x86 => 'ƒ',
        0x87 => '⁄',
        0x88 => '‹',
        0x89 => '›',
        0x8A => '−',
        0x8B => '‰',
        0x8C => '„',
        0x8D => '“',
        0x8E => '”',
        0x8F => '‘',
        0x90 => '’',
        0x91 => '‚',
        0x92 => '™',
        0x93 => 'ﬁ',
        0x94 => 'ﬂ',
        0xA0 => '€',
        b => char::from(b),
    }
}

/// Encode text as a PDF string: plain ASCII when possible, otherwise UTF-16BE.
fn pdf_text(s: &str) -> Object {
    if s.bytes().all(|b| (0x20..0x7f).contains(&b) || b == b'\n') {
        Object::String(s.as_bytes().to_vec(), StringFormat::Literal)
    } else {
        let mut v = vec![0xFE, 0xFF];
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_be_bytes());
        }
        Object::String(v, StringFormat::Hexadecimal)
    }
}

/// Plain text from XHTML rich content (`/RC`).
fn strip_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .trim()
        .to_owned()
}

fn hex(r: f32, g: f32, b: f32) -> String {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", c(r), c(g), c(b))
}

/// Convert a PDF date (`D:YYYYMMDDHHmmSSOHH'mm'`) to RFC 3339. Unparseable input is
/// returned unchanged.
pub fn pdf_date(s: &str) -> String {
    let t = s.trim().trim_start_matches("D:");
    let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 4 {
        return s.to_owned();
    }
    let part = |a: usize, b: usize, default: &str| digits.get(a..b).unwrap_or(default).to_owned();
    let (y, mo, d) = (part(0, 4, "0000"), part(4, 6, "01"), part(6, 8, "01"));
    let (h, mi, se) = (part(8, 10, "00"), part(10, 12, "00"), part(12, 14, "00"));
    let rest = &t[digits.len()..];
    let tz = match rest.chars().next() {
        Some('Z') | None => "Z".to_owned(),
        Some(sign @ ('+' | '-')) => {
            let tzd: String = rest[1..].chars().filter(char::is_ascii_digit).collect();
            let th = tzd.get(0..2).unwrap_or("00");
            let tm = tzd.get(2..4).unwrap_or("00");
            format!("{sign}{th}:{tm}")
        }
        _ => "Z".to_owned(),
    };
    format!("{y}-{mo}-{d}T{h}:{mi}:{se}{tz}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_pdf() -> Vec<u8> {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        let info = doc.add_object(dictionary! { "Author" => Object::string_literal("someone") });
        doc.trailer.set("Info", info);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn write_then_read_round_trips_with_replies() {
        let pdf = blank_pdf();
        let mut hl = AnnotationSpec::new(1, "Highlight", [100.0, 700.0, 200.0, 712.0]);
        hl.quads = vec![[100.0, 712.0, 200.0, 712.0, 100.0, 700.0, 200.0, 700.0]];
        hl.contents = Some("Cite the field study — here".into());
        hl.author = Some("Prof. Hale".into());
        hl.nm = Some("abc-1".into());
        hl.color = Some([1.0, 0.83, 0.0]);
        let pdf = write_annotations(&pdf, &[hl]).unwrap();
        let mut reply = AnnotationSpec::new(1, "Text", [200.0, 700.0, 220.0, 720.0]);
        reply.contents = Some("Done.".into());
        reply.in_reply_to = Some((1, 0));
        let pdf = write_annotations(&pdf, &[reply]).unwrap();

        let (boxes, annots) = read_annotations(&pdf, Limits::default()).unwrap();
        assert_eq!(boxes[0].media, [0.0, 0.0, 612.0, 792.0]);
        assert_eq!(annots.len(), 2);
        assert_eq!(annots[0].subtype, "Highlight");
        assert_eq!(annots[0].contents, "Cite the field study — here");
        assert_eq!(annots[0].author.as_deref(), Some("Prof. Hale"));
        assert_eq!(annots[0].nm.as_deref(), Some("abc-1"));
        assert_eq!(annots[0].quads.len(), 1);
        assert_eq!(annots[0].color.as_deref(), Some("#ffd400"));
        assert_eq!(annots[1].in_reply_to, Some(0));
    }

    #[test]
    fn strips_metadata() {
        let pdf = strip_metadata(&blank_pdf()).unwrap();
        let doc = Document::load_mem(&pdf).unwrap();
        assert!(doc.trailer.get(b"Info").is_err());
    }

    #[test]
    fn rejects_garbage_and_limits() {
        assert!(read_annotations(b"not a pdf", Limits::default()).is_err());
        let small = Limits {
            max_bytes: 10,
            ..Limits::default()
        };
        assert!(matches!(
            read_annotations(&blank_pdf(), small),
            Err(Error::LimitExceeded { .. })
        ));
    }

    #[test]
    fn dates_and_strings() {
        assert_eq!(
            pdf_date("D:20261124180300+01'00'"),
            "2026-11-24T18:03:00+01:00"
        );
        assert_eq!(pdf_date("D:20261124"), "2026-11-24T00:00:00Z");
        assert_eq!(pdf_date("garbage"), "garbage");
        assert_eq!(
            decode_text_string(&[0xFE, 0xFF, 0x00, 0x41, 0x20, 0x14]),
            "A—"
        );
        assert_eq!(
            strip_xml("<body><p>Hello &amp; bye</p></body>"),
            "Hello & bye"
        );
    }

    #[test]
    fn coordinates_flip() {
        let b = PageBox {
            page: 1,
            media: [0.0, 0.0, 612.0, 792.0],
        };
        assert_eq!(b.to_top_left(72.0, 720.0), (72.0, 72.0));
        assert_eq!(b.from_top_left(72.0, 72.0), (72.0, 720.0));
    }
}
