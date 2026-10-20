//! Plain-text converter. Stub for the trait-scaffolding commit; the
//! paragraph-splitting reader and space-aligned writer land in a later
//! commit.

use smelt_detect::Format;

use crate::error::{ReadError, WriteError};
use crate::idm::Document;
use crate::{Reader, Writer};

/// The plain-text reader (blank-line paragraph split -> IDM). Not yet
/// implemented.
pub struct TextReader;

/// The plain-text writer (IDM -> plain text). Not yet implemented.
pub struct TextWriter;

impl Reader for TextReader {
    fn read(&self, _input: &[u8], fmt: Format) -> Result<Document, ReadError> {
        Err(ReadError::Unsupported(format!("text ({})", fmt.name())))
    }
}

impl Writer for TextWriter {
    fn write(&self, _doc: &Document, fmt: Format) -> Result<Vec<u8>, WriteError> {
        Err(WriteError::Unsupported(format!("text ({})", fmt.name())))
    }
}
