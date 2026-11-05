//! PDF converter: PDF text extraction and text-layer writing.

use smelt_detect::Format;

use crate::error::{ReadError, WriteError};
use crate::idm::{Block, Document, Inline, Metadata};
use crate::{Reader, Writer};

/// The pdf reader.
pub struct PdfReader;

/// The pdf writer.
pub struct PdfWriter;

impl Reader for PdfReader {
    fn read(&self, input: &[u8], _fmt: Format) -> Result<Document, ReadError> {
        let _doc = pdf::file::FileOptions::cached()
            .load(input)
            .map_err(|e| ReadError::Parse {
                format: "pdf".to_string(),
                message: format!("pdf parse error: {e}"),
            })?;
        let mut text_parts: Vec<String> = Vec::new();
        if text_parts.is_empty() {
            let mut result = String::new();
            let bytes = input;
            let mut i = 0;
            while i < bytes.len().saturating_sub(4) {
                if &bytes[i..i + 2] == b"(" {
                    let mut j = i + 1;
                    let mut depth = 0;
                    while j < bytes.len() {
                        if bytes[j] == b'\\' && j + 1 < bytes.len() {
                            j += 2;
                            continue;
                        }
                        if bytes[j] == b'(' {
                            depth += 1;
                            j += 1;
                            continue;
                        }
                        if bytes[j] == b')' {
                            if depth == 0 {
                                break;
                            }
                            depth -= 1;
                            j += 1;
                            continue;
                        }
                        j += 1;
                    }
                    if j > i + 1 {
                        let slice = &bytes[i + 1..j];
                        let mut s = String::new();
                        let mut k = 0;
                        while k < slice.len() {
                            if slice[k] == b'\\' && k + 1 < slice.len() {
                                match slice[k + 1] {
                                    b'n' => s.push('\n'),
                                    b'r' => s.push('\r'),
                                    b't' => s.push('\t'),
                                    b'(' => s.push('('),
                                    b')' => s.push(')'),
                                    b'\\' => s.push('\\'),
                                    c => s.push(c as char),
                                }
                                k += 2;
                                continue;
                            }
                            if slice[k] >= 32 && slice[k] < 127 {
                                s.push(slice[k] as char);
                            }
                            k += 1;
                        }
                        if !s.trim().is_empty() {
                            result.push_str(&s);
                            result.push('\n');
                        }
                    }
                    i = j + 1;
                    continue;
                }
                i += 1;
            }
            if result.trim().is_empty() {
                return Err(ReadError::Parse {
                    format: "pdf".to_string(),
                    message: "no text content extracted from pdf".to_string(),
                });
            }
            return Ok(Document {
                meta: Metadata::default(),
                blocks: vec![Block::Paragraph {
                    inlines: vec![Inline::Text(result)],
                }],
            });
        }
        let all_text = text_parts.join("\n\n");
        let mut blocks: Vec<Block> = Vec::new();
        for para in all_text.split("\n\n") {
            let para = para.trim();
            if para.is_empty() {
                continue;
            }
            blocks.push(Block::Paragraph {
                inlines: vec![Inline::Text(para.to_string())],
            });
        }
        if blocks.is_empty() {
            blocks.push(Block::Paragraph {
                inlines: vec![Inline::Text(all_text)],
            });
        }
        Ok(Document {
            meta: Metadata::default(),
            blocks,
        })
    }
}

impl Writer for PdfWriter {
    fn write(&self, doc: &Document, _fmt: Format) -> Result<Vec<u8>, WriteError> {
        use lopdf::{Dictionary, Document as LopdfDoc, Object, Stream, content};

        let mut doc_pdf = LopdfDoc::with_version("1.5");
        let pages_id = doc_pdf.new_object_id();
        let font_id = doc_pdf.add_object(Dictionary::from_iter(vec![
            ("Type", "Font".into()),
            ("Subtype", "Type1".into()),
            ("BaseFont", "Helvetica".into()),
        ]));
        let resources_id = doc_pdf.add_object(Dictionary::from_iter(vec![(
            "Font",
            Dictionary::from_iter(vec![("F1", font_id.into())]).into(),
        )]));

        let mut text = String::new();
        for block in &doc.blocks {
            text.push_str(&block_to_text(block));
            text.push_str("\n\n");
        }
        let escaped = text
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)");
        let content_str = format!("BT /F1 12 Tf 50 750 Td ({}) Tj ET", escaped);
        let content_id =
            doc_pdf.add_object(Stream::new(Dictionary::new(), content_str.into_bytes()));

        let page_id = doc_pdf.add_object(Dictionary::from_iter(vec![
            ("Type", "Page".into()),
            ("Parent", pages_id.into()),
            ("Contents", content_id.into()),
            (
                "MediaBox",
                vec![0.into(), 0.into(), 595.into(), 842.into()].into(),
            ),
            ("Resources", resources_id.into()),
        ]));
        let pages_id_obj = doc_pdf.add_object(Dictionary::from_iter(vec![
            ("Type", "Pages".into()),
            ("Kids", vec![page_id.into()].into()),
            ("Count", 1.into()),
        ]));
        let catalog_id = doc_pdf.add_object(Dictionary::from_iter(vec![
            ("Type", "Catalog".into()),
            ("Pages", pages_id_obj.into()),
        ]));
        doc_pdf.trailer.set("Root", catalog_id);
        let mut buffer = Vec::new();
        doc_pdf
            .save_to(&mut buffer)
            .map_err(|e| WriteError::Serialize {
                format: "pdf".to_string(),
                message: format!("pdf write error: {e}"),
            })?;
        Ok(buffer)
    }
}

fn block_to_text(block: &Block) -> String {
    match block {
        Block::Heading { inlines, .. } => inlines_to_text(inlines),
        Block::Paragraph { inlines } => inlines_to_text(inlines),
        Block::List { items, .. } => items
            .iter()
            .map(|item_blocks| {
                item_blocks
                    .iter()
                    .map(block_to_text)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Block::CodeBlock { text, .. } => text.clone(),
        Block::Table { .. } => String::new(),
        Block::Image { alt, .. } => alt.clone(),
        Block::Rule => "---".to_string(),
        Block::Blockquote { blocks } => blocks
            .iter()
            .map(block_to_text)
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn inlines_to_text(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => s.push_str(t),
            Inline::Code(t) => s.push_str(t),
            Inline::Strong(inner) | Inline::Emph(inner) | Inline::Strikethrough(inner) => {
                s.push_str(&inlines_to_text(inner))
            }
            Inline::Link { inlines, .. } => s.push_str(&inlines_to_text(inlines)),
        }
    }
    s
}
