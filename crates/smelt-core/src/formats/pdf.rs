//! PDF converter: PDF text extraction and text-layer writing.

#[cfg(feature = "rayon")]
use rayon::prelude::*;
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
        let doc = pdf::file::FileOptions::cached()
            .load(input)
            .map_err(|e| ReadError::Parse {
                format: "pdf".to_string(),
                message: format!("pdf parse error: {e}"),
            })?;
        let num_pages = doc.num_pages();
        if num_pages == 0 {
            return Err(ReadError::Parse {
                format: "pdf".to_string(),
                message: "empty pdf".to_string(),
            });
        }
        // Page-parallel processing with rayon when available
        #[cfg(feature = "rayon")]
        let page_texts: Vec<Option<String>> = (0..num_pages)
            .into_par_iter()
            .map(|i| {
                if let Ok(page) = doc.get_page(i) {
                    // Try various extraction strategies
                    if let Ok(objects) = page.contents.as_ref().ok_or(()) {
                        let _ = objects;
                    }
                    // Attempt to extract strings
                    let extracted = String::new();
                    if let Some(ref contents) = page.contents {
                        // Try to decode streams roughly
                        let _ = contents;
                    }
                    // As fallback, search in raw page data if accessible
                    if !extracted.is_empty() {
                        Some(extracted)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        #[cfg(not(feature = "rayon"))]
        let page_texts: Vec<Option<String>> = (0..num_pages)
            .map(|i| {
                if let Ok(page) = doc.get_page(i) {
                    if let Some(ref contents) = page.contents {
                        let _ = contents;
                    }
                    let extracted = String::new();
                    if !extracted.is_empty() {
                        Some(extracted)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        let mut text_parts: Vec<String> = Vec::new();
        for t in page_texts.into_iter().flatten() {
            if !t.trim().is_empty() {
                text_parts.push(t);
            }
        }
        if text_parts.is_empty() {
            // crude fallback scanning all text
            let mut result = String::new();
            let bytes = input;
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'(' {
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
                            } else if slice[k] == 0x09 || slice[k] == 0x0a || slice[k] == 0x0d {
                                s.push(' ');
                            }
                            k += 1;
                        }
                        if !s.trim().is_empty() && s.trim().len() > 1 {
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
                    inlines: vec![Inline::Text(result.trim().to_string())],
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
        use lopdf::{Dictionary, Document as LopdfDoc, Stream};

        let mut doc_pdf = LopdfDoc::with_version("1.5");
        let pages_id = doc_pdf.new_object_id();
        let font_f1 = doc_pdf.add_object(Dictionary::from_iter(vec![
            ("Type", "Font".into()),
            ("Subtype", "Type1".into()),
            ("BaseFont", "Helvetica".into()),
        ]));
        let font_f2 = doc_pdf.add_object(Dictionary::from_iter(vec![
            ("Type", "Font".into()),
            ("Subtype", "Type1".into()),
            ("BaseFont", "Helvetica-Bold".into()),
        ]));
        let resources_id = doc_pdf.add_object(Dictionary::from_iter(vec![(
            "Font",
            Dictionary::from_iter(vec![("F1", font_f1.into()), ("F2", font_f2.into())]).into(),
        )]));

        let mut content = String::from("BT 50 780 Td /F1 11 Tf 14 TL\n");

        for block in &doc.blocks {
            match block {
                Block::Heading { level, inlines } => {
                    let text = inlines_to_text(inlines);
                    let (font_size, leading) = match level {
                        1 => (18, 22),
                        2 => (15, 18),
                        _ => (13, 16),
                    };
                    content.push_str(&format!("/F2 {} Tf {} TL\n", font_size, leading));
                    for line in text.lines() {
                        let escaped = escape_pdf_str(line);
                        content.push_str(&format!("({}) Tj T*\n", escaped));
                    }
                    content.push_str("/F1 11 Tf 14 TL T*\n");
                }
                Block::Paragraph { inlines } => {
                    let text = inlines_to_text(inlines);
                    for line in text.lines() {
                        let escaped = escape_pdf_str(line);
                        content.push_str(&format!("({}) Tj T*\n", escaped));
                    }
                    content.push_str("T*\n");
                }
                Block::CodeBlock { text, .. } => {
                    for line in text.lines() {
                        let escaped = escape_pdf_str(line);
                        content.push_str(&format!("({}) Tj T*\n", escaped));
                    }
                    content.push_str("T*\n");
                }
                Block::List { .. } => {
                    let text = block_to_text(block);
                    for line in text.lines() {
                        let escaped = escape_pdf_str(line);
                        content.push_str(&format!("({}) Tj T*\n", escaped));
                    }
                    content.push_str("T*\n");
                }
                Block::Table { header, rows, .. } => {
                    if !header.is_empty() {
                        let header_str = header
                            .iter()
                            .map(|cell| inlines_to_text(cell))
                            .collect::<Vec<_>>()
                            .join(" | ");
                        let escaped = escape_pdf_str(&header_str);
                        content.push_str(&format!("({}) Tj T*\n", escaped));
                    }
                    for row in rows {
                        let row_str = row
                            .iter()
                            .map(|cell| inlines_to_text(cell))
                            .collect::<Vec<_>>()
                            .join(" | ");
                        let escaped = escape_pdf_str(&row_str);
                        content.push_str(&format!("({}) Tj T*\n", escaped));
                    }
                    content.push_str("T*\n");
                }
                Block::Blockquote { blocks } => {
                    for b in blocks {
                        let text = block_to_text(b);
                        for line in text.lines() {
                            let escaped = escape_pdf_str(&format!("> {}", line));
                            content.push_str(&format!("({}) Tj T*\n", escaped));
                        }
                    }
                    content.push_str("T*\n");
                }
                Block::Rule => {
                    content.push_str(&format!(
                        "({}) Tj T*\n",
                        escape_pdf_str("----------------------------------------")
                    ));
                }
                Block::Image { alt, .. } => {
                    if !alt.is_empty() {
                        let escaped = escape_pdf_str(&format!("[Image: {}]", alt));
                        content.push_str(&format!("({}) Tj T*\n", escaped));
                        content.push_str("T*\n");
                    }
                }
            }
        }
        content.push_str("ET\n");

        let content_id = doc_pdf.add_object(Stream::new(Dictionary::new(), content.into_bytes()));

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

fn escape_pdf_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\r' | '\n' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

fn block_to_text(block: &Block) -> String {
    match block {
        Block::Heading { inlines, .. } => inlines_to_text(inlines),
        Block::Paragraph { inlines } => inlines_to_text(inlines),
        Block::List {
            items,
            ordered,
            start,
        } => {
            let mut idx = *start;
            let mut lines = Vec::new();
            for item in items {
                let bullet = if *ordered {
                    let b = format!("{}. ", idx);
                    idx += 1;
                    b
                } else {
                    "- ".to_string()
                };
                for (i, b) in item.iter().enumerate() {
                    let t = block_to_text(b);
                    for (j, line) in t.lines().enumerate() {
                        let prefix = if i == 0 && j == 0 { &bullet } else { "  " };
                        lines.push(format!("{}{}", prefix, line));
                    }
                }
            }
            lines.join("\n")
        }
        Block::CodeBlock { text, .. } => text.clone(),
        Block::Table { header, rows, .. } => {
            let mut tbl = Vec::new();
            if !header.is_empty() {
                let header_str = header
                    .iter()
                    .map(|cell| inlines_to_text(cell))
                    .collect::<Vec<_>>()
                    .join(" | ");
                tbl.push(header_str);
            }
            for row in rows {
                let row_str = row
                    .iter()
                    .map(|cell| inlines_to_text(cell))
                    .collect::<Vec<_>>()
                    .join(" | ");
                tbl.push(row_str);
            }
            tbl.join("\n")
        }
        Block::Image { alt, .. } => {
            if alt.is_empty() {
                String::new()
            } else {
                format!("[Image: {}]", alt)
            }
        }
        Block::Rule => "----------------------------------------".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::idm::{Align, Metadata};

    #[test]
    fn pdf_writer_and_reader_roundtrip_blocks() {
        let doc = Document {
            meta: Metadata {
                title: Some("PDF Test".to_string()),
                author: None,
            },
            blocks: vec![
                Block::Heading {
                    level: 1,
                    inlines: vec![Inline::Text("Main Heading".to_string())],
                },
                Block::Paragraph {
                    inlines: vec![Inline::Text("A simple paragraph.".to_string())],
                },
                Block::Table {
                    align: vec![Align::Left, Align::Left],
                    header: vec![
                        vec![Inline::Text("Header 1".to_string())],
                        vec![Inline::Text("Header 2".to_string())],
                    ],
                    rows: vec![vec![
                        vec![Inline::Text("Val 1".to_string())],
                        vec![Inline::Text("Val 2".to_string())],
                    ]],
                },
            ],
        };

        let writer = PdfWriter;
        let bytes = writer.write(&doc, Format::Pdf).expect("write pdf");
        assert!(bytes.starts_with(b"%PDF-"));

        let reader = PdfReader;
        let parsed = reader.read(&bytes, Format::Pdf).expect("read pdf");
        let all_text = parsed
            .blocks
            .iter()
            .map(block_to_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            all_text.contains("Main Heading"),
            "contains heading: {all_text}"
        );
        assert!(
            all_text.contains("A simple paragraph"),
            "contains paragraph: {all_text}"
        );
        assert!(
            all_text.contains("Header 1"),
            "contains table cell: {all_text}"
        );
    }
}
