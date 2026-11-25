//! PDF access for proofback.
//!
//! - [`annots`]: read and write annotation dictionaries with `lopdf` (pure Rust).
//!   This covers everything PDFium's public API cannot do safely: unique names,
//!   reply chains (`/IRT`), writing reply notes, and stripping metadata.
//! - [`text`]: positioned page text from PDFium, used to read the words under a
//!   highlight even when the annotating app did not store them.
//!
//! Coordinates follow PDF conventions: points, origin at the bottom-left of the
//! page's media box.

pub mod annots;
pub mod text;

pub use annots::{
    AnnotationSpec, Limits, PageBox, PdfAnnotation, read_annotations, strip_metadata,
    write_annotations,
};
pub use text::{MarkedText, PageText, TextChar, TextExtractor, load_pdfium, marked_text};

/// Errors from the PDF layer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read the PDF: {0}")]
    Parse(String),
    #[error("the PDF is encrypted; remove the password and try again")]
    Encrypted,
    #[error("the PDF exceeds the {what} limit ({limit})")]
    LimitExceeded { what: &'static str, limit: u64 },
    #[error(
        "PDFium is not available: {0}. Set PROOFBACK_PDFIUM to the library path or run `cargo xtask pdfium`"
    )]
    PdfiumMissing(String),
    #[error("PDFium error: {0}")]
    Pdfium(String),
    #[error("could not write the PDF: {0}")]
    Write(String),
}
