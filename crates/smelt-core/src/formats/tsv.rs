use smelt_detect::Format;

use crate::error::{ReadError, WriteError};
use crate::{Document, Reader, Writer};

use super::delimited;

/// Native TSV reader: the first record becomes the IDM table header.
pub struct TsvReader;

/// Native TSV writer: accepts exactly one IDM table.
pub struct TsvWriter;

impl Reader for TsvReader {
    fn read(&self, input: &[u8], _fmt: Format) -> Result<Document, ReadError> {
        delimited::read(input, '\t', "tsv")
    }
}

impl Writer for TsvWriter {
    fn write(&self, doc: &Document, _fmt: Format) -> Result<Vec<u8>, WriteError> {
        delimited::write(doc, '\t', "tsv")
    }
}
