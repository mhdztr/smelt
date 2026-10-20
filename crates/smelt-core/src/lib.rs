//! smelt-core: the Intermediate Document Model (IDM), reader/writer traits,
//! and native format converters. See `docs/idm-spec.md` for the model spec.
//!
//! Data flow: bytes --[Reader]--> `Document` (IDM) --[transform]--> bytes
//! via `Writer`. Readers and writers are looked up with [`reader_for`] and
//! [`writer_for`]; dispatch is static in M0 and grows engine adapters in M1.

pub mod error;
pub mod formats;
pub mod idm;

pub use error::{ReadError, WriteError};
pub use idm::{Align, Block, Document, Inline, Metadata};

/// Format enum lives in the leaf crate `smelt-detect`; re-exported here so
/// callers only need `smelt-core` for conversion work.
pub use smelt_detect::{Confidence, Detection, Format};

/// Parses input bytes of a known format into an IDM document.
///
/// `fmt` tells the reader how to interpret the bytes; readers that cannot
/// handle their `fmt` (should never happen for registered pairs) return
/// [`ReadError::Unsupported`].
pub trait Reader: Send + Sync {
    fn read(&self, input: &[u8], fmt: Format) -> Result<Document, ReadError>;
}

/// Serializes an IDM document into bytes of a known format.
pub trait Writer: Send + Sync {
    fn write(&self, doc: &Document, fmt: Format) -> Result<Vec<u8>, WriteError>;
}

/// The reader for a format, if smelt has a native one (M0: markdown, html,
/// text). `None` means unsupported -- the CLI maps that to exit code 3.
pub fn reader_for(fmt: Format) -> Option<&'static dyn Reader> {
    match fmt {
        Format::Markdown => Some(&formats::markdown::MarkdownReader),
        Format::Html => Some(&formats::html::HtmlReader),
        Format::Text => Some(&formats::text::TextReader),
        _ => None,
    }
}

/// The writer for a format, if smelt has a native one (M0: markdown, html,
/// text). `None` means unsupported -- the CLI maps that to exit code 3.
pub fn writer_for(fmt: Format) -> Option<&'static dyn Writer> {
    match fmt {
        Format::Markdown => Some(&formats::markdown::MarkdownWriter),
        Format::Html => Some(&formats::html::HtmlWriter),
        Format::Text => Some(&formats::text::TextWriter),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_covers_native_formats_only() {
        for fmt in [Format::Markdown, Format::Html, Format::Text] {
            assert!(reader_for(fmt).is_some(), "reader for {fmt:?}");
            assert!(writer_for(fmt).is_some(), "writer for {fmt:?}");
        }
        for fmt in [
            Format::Pdf,
            Format::Docx,
            Format::Odt,
            Format::Xlsx,
            Format::Pptx,
            Format::Epub,
            Format::Rtf,
            Format::Csv,
            Format::Tsv,
            Format::Unknown,
        ] {
            assert!(reader_for(fmt).is_none(), "reader for {fmt:?}");
            assert!(writer_for(fmt).is_none(), "writer for {fmt:?}");
        }
    }
}
