//! HTML converter. Stub for the trait-scaffolding commit; the scraper
//! reader and the hand-rolled writer land in a later commit.

use smelt_detect::Format;

use crate::error::{ReadError, WriteError};
use crate::idm::Document;
use crate::{Reader, Writer};

/// The html reader (scraper DOM walk -> IDM). Not yet implemented.
pub struct HtmlReader;

/// The html writer (IDM -> html). Not yet implemented.
pub struct HtmlWriter;

impl Reader for HtmlReader {
    fn read(&self, _input: &[u8], fmt: Format) -> Result<Document, ReadError> {
        Err(ReadError::Unsupported(format!("html ({})", fmt.name())))
    }
}

impl Writer for HtmlWriter {
    fn write(&self, _doc: &Document, fmt: Format) -> Result<Vec<u8>, WriteError> {
        Err(WriteError::Unsupported(format!("html ({})", fmt.name())))
    }
}
