//! Shared CSV/TSV parsing and serialization.
//!
//! A delimited file maps to one IDM table: the first record is the header and
//! every later record is a body row. The parser is deliberately strict about
//! record width so malformed tabular input fails next to the source instead of
//! being silently padded or truncated.

use std::fmt::Write as _;

use crate::error::{ReadError, WriteError};
use crate::idm::{Align, Block, Document, Inline, Metadata};

pub(crate) fn read(input: &[u8], delimiter: char, format: &str) -> Result<Document, ReadError> {
    let text = std::str::from_utf8(input).map_err(|_| ReadError::NotUtf8)?;
    let records = parse_records(text, delimiter, format)?;
    if records.is_empty() {
        return Ok(Document {
            meta: Metadata::default(),
            blocks: Vec::new(),
        });
    }

    let column_count = records[0].len();
    if column_count == 0 {
        return Err(parse_error(format, "header has no fields"));
    }
    for (index, record) in records.iter().enumerate().skip(1) {
        if record.len() != column_count {
            return Err(parse_error(
                format,
                &format!(
                    "row {} has {} fields, expected {}",
                    index + 1,
                    record.len(),
                    column_count
                ),
            ));
        }
    }

    let header = records[0]
        .iter()
        .map(|field| field_inlines(field))
        .collect();
    let rows = records[1..]
        .iter()
        .map(|record| record.iter().map(|field| field_inlines(field)).collect())
        .collect();
    Ok(Document {
        meta: Metadata::default(),
        blocks: vec![Block::Table {
            header,
            rows,
            align: vec![Align::None; column_count],
        }],
    })
}

fn field_inlines(field: &str) -> Vec<Inline> {
    if field.is_empty() {
        Vec::new()
    } else {
        vec![Inline::Text(field.to_owned())]
    }
}

pub(crate) fn write(doc: &Document, delimiter: char, format: &str) -> Result<Vec<u8>, WriteError> {
    let [Block::Table { header, rows, .. }] = doc.blocks.as_slice() else {
        return Err(write_error(
            format,
            "output requires exactly one table block",
        ));
    };
    if header.is_empty() {
        return Err(write_error(
            format,
            "table must contain at least one column",
        ));
    }

    let mut out = String::new();
    write_record(&mut out, header, delimiter);
    for (index, row) in rows.iter().enumerate() {
        if row.len() != header.len() {
            return Err(write_error(
                format,
                &format!(
                    "row {} has {} cells, expected {}",
                    index + 2,
                    row.len(),
                    header.len()
                ),
            ));
        }
        write_record(&mut out, row, delimiter);
    }
    Ok(out.into_bytes())
}

fn parse_records(
    input: &str,
    delimiter: char,
    format: &str,
) -> Result<Vec<Vec<String>>, ReadError> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut chars = input.chars().peekable();
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut ended_after_newline = false;
    let mut row_number = 1;

    while let Some(ch) = chars.next() {
        ended_after_newline = false;
        if in_quotes {
            match ch {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => {
                    in_quotes = false;
                    after_quote = true;
                }
                other => field.push(other),
            }
            continue;
        }

        if after_quote {
            match ch {
                c if c == delimiter => {
                    record.push(std::mem::take(&mut field));
                    after_quote = false;
                }
                '\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    after_quote = false;
                    ended_after_newline = true;
                    row_number += 1;
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    after_quote = false;
                    ended_after_newline = true;
                    row_number += 1;
                }
                _ => {
                    return Err(parse_error(
                        format,
                        &format!("unexpected character after closing quote on row {row_number}"),
                    ));
                }
            }
            continue;
        }

        match ch {
            '"' if field.is_empty() => in_quotes = true,
            '"' => {
                return Err(parse_error(
                    format,
                    &format!("unexpected quote inside an unquoted field on row {row_number}"),
                ));
            }
            c if c == delimiter => record.push(std::mem::take(&mut field)),
            '\n' => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                ended_after_newline = true;
                row_number += 1;
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                ended_after_newline = true;
                row_number += 1;
            }
            other => field.push(other),
        }
    }

    if in_quotes {
        return Err(parse_error(
            format,
            &format!("unterminated quoted field on row {row_number}"),
        ));
    }
    if !input.is_empty() && !ended_after_newline {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

fn write_record(out: &mut String, cells: &[Vec<Inline>], delimiter: char) {
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            out.push(delimiter);
        }
        let text = inline_text(cell);
        if needs_quotes(&text, delimiter) {
            out.push('"');
            out.push_str(&text.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(&text);
        }
    }
    out.push('\n');
}

fn needs_quotes(value: &str, delimiter: char) -> bool {
    value.contains(delimiter)
        || value.contains(['"', '\r', '\n'])
        || value
            .chars()
            .next()
            .is_some_and(|character| character.is_whitespace())
        || value
            .chars()
            .next_back()
            .is_some_and(|character| character.is_whitespace())
}

fn inline_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(text) | Inline::Code(text) => out.push_str(text),
            Inline::Strong(inner) | Inline::Emph(inner) | Inline::Strikethrough(inner) => {
                out.push_str(&inline_text(inner));
            }
            Inline::Link { dest, inlines, .. } => {
                out.push_str(&inline_text(inlines));
                let _ = write!(out, " ({dest})");
            }
        }
    }
    out
}

fn parse_error(format: &str, message: &str) -> ReadError {
    ReadError::Parse {
        format: format.to_owned(),
        message: message.to_owned(),
    }
}

fn write_error(format: &str, message: &str) -> WriteError {
    WriteError::Serialize {
        format: format.to_owned(),
        message: message.to_owned(),
    }
}
