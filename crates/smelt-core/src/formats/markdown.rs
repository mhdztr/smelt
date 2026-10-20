//! Markdown converter. Stub for the trait-scaffolding commit; the comrak
//! reader and writer land in the next commit.

use smelt_detect::Format;

use crate::error::{ReadError, WriteError};
use crate::idm::Document;
use crate::{Reader, Writer};

/// The markdown reader (comrak AST -> IDM). Not yet implemented.
pub struct MarkdownReader;

/// The markdown writer (IDM -> markdown). Not yet implemented.
pub struct MarkdownWriter;

impl Reader for MarkdownReader {
    fn read(&self, _input: &[u8], fmt: Format) -> Result<Document, ReadError> {
        Err(ReadError::Unsupported(format!("markdown ({})", fmt.name())))
    }
}

impl Writer for MarkdownWriter {
    fn write(&self, _doc: &Document, fmt: Format) -> Result<Vec<u8>, WriteError> {
        Err(WriteError::Unsupported(format!(
            "markdown ({})",
            fmt.name()
        )))
    }
}
