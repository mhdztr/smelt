use smelt_detect::Format;

use crate::error::{ReadError, WriteError};
use crate::{Document, Reader, Writer};

use super::delimited;

/// Native CSV reader: the first record becomes the IDM table header.
pub struct CsvReader;

/// Native CSV writer: accepts exactly one IDM table.
pub struct CsvWriter;

impl Reader for CsvReader {
    fn read(&self, input: &[u8], _fmt: Format) -> Result<Document, ReadError> {
        delimited::read(input, ',', "csv")
    }
}

impl Writer for CsvWriter {
    fn write(&self, doc: &Document, _fmt: Format) -> Result<Vec<u8>, WriteError> {
        delimited::write(doc, ',', "csv")
    }
}
