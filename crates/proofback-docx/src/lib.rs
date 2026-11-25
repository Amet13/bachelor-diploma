//! Word round trip.
//!
//! [`export`] writes a DOCX in which every paragraph carries a hidden bookmark
//! (`pb_<file>_<start>_<end>`) naming its source byte range; the list of files is
//! stored in a custom XML part, which Word preserves. [`import`] reads tracked
//! insertions and deletions and comments straight from the XML and resolves each
//! through the nearest bookmark plus text alignment, so Word feedback becomes
//! ordinary items. A DOCX without bookmarks still works in text-only mode.

mod export;
mod import;

pub use export::{Paragraph, add_bookmarks, export, paragraphs};
pub use import::{Change, ChangeKind, import, read_changes};

/// Errors from the Word layer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a readable Word document: {0}")]
    Archive(String),
    #[error("the Word document exceeds the {what} limit")]
    Limit { what: &'static str },
    #[error("invalid Word XML: {0}")]
    Xml(String),
}

/// Upper bounds for untrusted documents.
pub(crate) const MAX_PART_BYTES: u64 = 64 << 20;
pub(crate) const MAX_ENTRIES: usize = 5000;

/// Visible stand-in for a reference, citation or inline maths in the exported text.
/// On import, `⟨…⟩` matches the source placeholder.
pub(crate) const MARK_OPEN: char = '⟨';
pub(crate) const MARK_CLOSE: char = '⟩';

#[cfg(test)]
mod tests;
