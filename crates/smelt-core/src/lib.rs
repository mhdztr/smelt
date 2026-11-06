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

/// The reader for a format, if smelt has a native one compiled into this build.
/// `None` means unsupported -- the CLI maps that to exit code 3.
pub fn reader_for(fmt: Format) -> Option<&'static dyn Reader> {
    match fmt {
        #[cfg(feature = "markdown")]
        Format::Markdown => Some(&formats::markdown::MarkdownReader),
        #[cfg(feature = "html")]
        Format::Html => Some(&formats::html::HtmlReader),
        Format::Text => Some(&formats::text::TextReader),
        Format::Csv => Some(&formats::csv::CsvReader),
        Format::Tsv => Some(&formats::tsv::TsvReader),
        #[cfg(feature = "epub")]
        Format::Epub => Some(&formats::epub::EpubReader),
        #[cfg(feature = "pdf")]
        Format::Pdf => Some(&formats::pdf::PdfReader),
        _ => None,
    }
}

/// The writer for a format, if smelt has a native one compiled into this build.
/// `None` means unsupported -- the CLI maps that to exit code 3.
pub fn writer_for(fmt: Format) -> Option<&'static dyn Writer> {
    match fmt {
        #[cfg(feature = "markdown")]
        Format::Markdown => Some(&formats::markdown::MarkdownWriter),
        #[cfg(feature = "html")]
        Format::Html => Some(&formats::html::HtmlWriter),
        Format::Text => Some(&formats::text::TextWriter),
        Format::Csv => Some(&formats::csv::CsvWriter),
        Format::Tsv => Some(&formats::tsv::TsvWriter),
        #[cfg(feature = "epub")]
        Format::Epub => Some(&formats::epub::EpubWriter),
        #[cfg(feature = "pdf")]
        Format::Pdf => Some(&formats::pdf::PdfWriter),
        _ => None,
    }
}

/// Parse `input` with the native reader for `fmt` and enforce the IDM
/// invariants (see [`Document::validate`]) before handing the document on.
///
/// This is the entry point readers are meant to be driven through: a reader
/// that produces a structurally invalid document fails here, next to the
/// cause, instead of panicking later in a writer that trusts the model.
pub fn read_document(fmt: Format, input: &[u8]) -> Result<Document, ReadError> {
    let name = || fmt.name().to_lowercase();
    let reader = reader_for(fmt).ok_or_else(|| ReadError::Unsupported(name()))?;
    let doc = reader.read(input, fmt)?;
    doc.validate().map_err(|message| ReadError::Parse {
        format: name(),
        message,
    })?;
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_covers_native_formats_only() {
        for fmt in [Format::Text, Format::Csv, Format::Tsv] {
            assert!(reader_for(fmt).is_some(), "reader for {fmt:?}");
            assert!(writer_for(fmt).is_some(), "writer for {fmt:?}");
        }
        #[cfg(feature = "markdown")]
        {
            assert!(reader_for(Format::Markdown).is_some());
            assert!(writer_for(Format::Markdown).is_some());
        }
        #[cfg(feature = "html")]
        {
            assert!(reader_for(Format::Html).is_some());
            assert!(writer_for(Format::Html).is_some());
        }
        #[cfg(feature = "epub")]
        {
            assert!(reader_for(Format::Epub).is_some());
            assert!(writer_for(Format::Epub).is_some());
        }
        #[cfg(feature = "pdf")]
        {
            assert!(reader_for(Format::Pdf).is_some());
            assert!(writer_for(Format::Pdf).is_some());
        }
        for fmt in [
            Format::Docx,
            Format::Odt,
            Format::Ods,
            Format::Odp,
            Format::Xlsx,
            Format::Pptx,
            Format::Rtf,
            Format::Unknown,
        ] {
            assert!(reader_for(fmt).is_none(), "reader for {fmt:?}");
            assert!(writer_for(fmt).is_none(), "writer for {fmt:?}");
        }
    }

    /// `read_document` returns a document that already satisfies the IDM
    /// invariants: validation runs on the way out, so callers never receive
    /// a structurally invalid `Document`.
    #[test]
    #[cfg(feature = "markdown")]
    fn read_document_validates_the_parsed_result() {
        let doc = read_document(Format::Markdown, b"# Head\n\nbody\n").expect("markdown parses");
        assert_eq!(doc.validate(), Ok(()));
        assert_eq!(doc.blocks.len(), 2);
    }

    /// A format with no native reader is a capability gap, not malformed
    /// input: `read_document` must surface [`ReadError::Unsupported`] so the
    /// CLI keeps exit code 3, never `ReadError::Parse` (exit 1).
    #[test]
    fn read_document_reports_unsupported_without_a_reader() {
        let err = read_document(Format::Docx, b"not a docx").expect_err("unsupported");
        assert!(err.is_unsupported());
    }
}
