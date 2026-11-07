//! Native DOCX converter: reads and writes WordprocessingML via OpenXML ZIP container.
//! Zero external runtime required (LibreOffice not needed).

use std::io::{Cursor, Read, Write};

use smelt_detect::Format;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::error::{ReadError, WriteError};
use crate::idm::{Align, Block, Document, Inline, Metadata};
use crate::{Reader, Writer};

/// The native DOCX reader.
pub struct DocxReader;

/// The native DOCX writer.
pub struct DocxWriter;

impl Reader for DocxReader {
    fn read(&self, input: &[u8], _fmt: Format) -> Result<Document, ReadError> {
        let cursor = Cursor::new(input);
        let mut archive = ZipArchive::new(cursor).map_err(|e| ReadError::Parse {
            format: "docx".to_string(),
            message: format!("invalid docx archive: {e}"),
        })?;

        let mut meta = Metadata::default();

        // 1. Try reading docProps/core.xml for title and author
        if let Ok(mut core_file) = archive.by_name("docProps/core.xml") {
            let mut core_xml = String::new();
            if core_file.read_to_string(&mut core_xml).is_ok() {
                if let Some(title) = extract_tag(&core_xml, "dc:title")
                    && !title.is_empty()
                {
                    meta.title = Some(title);
                }
                if let Some(author) = extract_tag(&core_xml, "dc:creator")
                    && !author.is_empty()
                {
                    meta.author = Some(author);
                }
            }
        }

        // 2. Read word/document.xml
        let mut doc_file = archive
            .by_name("word/document.xml")
            .map_err(|e| ReadError::Parse {
                format: "docx".to_string(),
                message: format!("word/document.xml missing: {e}"),
            })?;

        let mut doc_xml = String::new();
        doc_file
            .read_to_string(&mut doc_xml)
            .map_err(|e| ReadError::Parse {
                format: "docx".to_string(),
                message: format!("failed to read document.xml: {e}"),
            })?;

        let blocks = parse_body(&doc_xml);

        Ok(Document { meta, blocks })
    }
}

impl Writer for DocxWriter {
    fn write(&self, doc: &Document, _fmt: Format) -> Result<Vec<u8>, WriteError> {
        let mut buffer = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(&mut buffer);

        let options_deflated = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        // [Content_Types].xml
        zip.start_file("[Content_Types].xml", options_deflated)
            .map_err(|e| WriteError::Serialize {
                format: "docx".to_string(),
                message: format!("zip error: {e}"),
            })?;
        zip.write_all(CONTENT_TYPES_XML.as_bytes())
            .map_err(|e| WriteError::Serialize {
                format: "docx".to_string(),
                message: format!("zip error: {e}"),
            })?;

        // _rels/.rels
        zip.start_file("_rels/.rels", options_deflated)
            .map_err(|e| WriteError::Serialize {
                format: "docx".to_string(),
                message: format!("zip error: {e}"),
            })?;
        zip.write_all(RELS_XML.as_bytes())
            .map_err(|e| WriteError::Serialize {
                format: "docx".to_string(),
                message: format!("zip error: {e}"),
            })?;

        // docProps/core.xml
        zip.start_file("docProps/core.xml", options_deflated)
            .map_err(|e| WriteError::Serialize {
                format: "docx".to_string(),
                message: format!("zip error: {e}"),
            })?;
        let core_xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
            <cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
            xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n  \
              <dc:title>{}</dc:title>\n  \
              <dc:creator>{}</dc:creator>\n\
            </cp:coreProperties>",
            xml_escape(doc.meta.title.as_deref().unwrap_or("")),
            xml_escape(doc.meta.author.as_deref().unwrap_or(""))
        );
        zip.write_all(core_xml.as_bytes())
            .map_err(|e| WriteError::Serialize {
                format: "docx".to_string(),
                message: format!("zip error: {e}"),
            })?;

        // word/document.xml
        zip.start_file("word/document.xml", options_deflated)
            .map_err(|e| WriteError::Serialize {
                format: "docx".to_string(),
                message: format!("zip error: {e}"),
            })?;
        let doc_xml = serialize_document(doc);
        zip.write_all(doc_xml.as_bytes())
            .map_err(|e| WriteError::Serialize {
                format: "docx".to_string(),
                message: format!("zip error: {e}"),
            })?;

        zip.finish().map_err(|e| WriteError::Serialize {
            format: "docx".to_string(),
            message: format!("zip finish error: {e}"),
        })?;

        Ok(buffer.into_inner())
    }
}

const CONTENT_TYPES_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\n  \
  <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\n  \
  <Default Extension=\"xml\" ContentType=\"application/xml\"/>\n  \
  <Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\n  \
  <Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>\n\
</Types>";

const RELS_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\n  \
  <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\n  \
  <Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>\n\
</Relationships>";

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].trim().to_string())
}

// ---------------- XML Parsing (Reader) ----------------

fn parse_body(xml: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let body_start = match xml.find("<w:body>") {
        Some(pos) => pos + 8,
        None => return blocks,
    };
    let body_end = xml.rfind("</w:body>").unwrap_or(xml.len());
    let body_content = &xml[body_start..body_end];

    let mut cursor = 0;
    while cursor < body_content.len() {
        let rest = &body_content[cursor..];
        let p_pos = rest.find("<w:p");
        let tbl_pos = rest.find("<w:tbl");

        match (p_pos, tbl_pos) {
            (Some(p), Some(tbl)) if tbl < p => {
                let start = cursor + tbl;
                if let Some(end_offset) = body_content[start..].find("</w:tbl>") {
                    let end = start + end_offset + 8;
                    if let Some(table) = parse_table(&body_content[start..end]) {
                        blocks.push(table);
                    }
                    cursor = end;
                } else {
                    break;
                }
            }
            (Some(p), _) => {
                let start = cursor + p;
                if let Some(end_offset) = body_content[start..].find("</w:p>") {
                    let end = start + end_offset + 6;
                    if let Some(block) = parse_paragraph(&body_content[start..end]) {
                        blocks.push(block);
                    }
                    cursor = end;
                } else {
                    break;
                }
            }
            (None, Some(tbl)) => {
                let start = cursor + tbl;
                if let Some(end_offset) = body_content[start..].find("</w:tbl>") {
                    let end = start + end_offset + 8;
                    if let Some(table) = parse_table(&body_content[start..end]) {
                        blocks.push(table);
                    }
                    cursor = end;
                } else {
                    break;
                }
            }
            (None, None) => break,
        }
    }

    blocks
}

fn parse_paragraph(p_xml: &str) -> Option<Block> {
    // Check for horizontal rule (bottom border on paragraph with no runs)
    if p_xml.contains("<w:bottom") && !p_xml.contains("<w:t") {
        return Some(Block::Rule);
    }

    // Check heading style: e.g. <w:pStyle w:val="Heading1"/> or "heading 1"
    let mut heading_level: Option<u8> = None;
    if let Some(idx) = p_xml.find("<w:pStyle ") {
        let rest = &p_xml[idx..];
        if let Some(val_idx) = rest.find("w:val=\"") {
            let val_str = &rest[val_idx + 7..];
            if let Some(quote_idx) = val_str.find('"') {
                let val = &val_str[..quote_idx];
                let lower = val.to_lowercase();
                if let Some(rest_num) = lower.strip_prefix("heading") {
                    let num = rest_num.trim();
                    if let Ok(lvl) = num.parse::<u8>() {
                        heading_level = Some(lvl.clamp(1, 6));
                    }
                } else if lower == "title" {
                    heading_level = Some(1);
                }
            }
        }
    }

    let inlines = parse_inlines(p_xml);
    if inlines.is_empty() {
        return None;
    }

    if let Some(lvl) = heading_level {
        Some(Block::Heading {
            level: lvl,
            inlines,
        })
    } else {
        Some(Block::Paragraph { inlines })
    }
}

fn parse_inlines(p_xml: &str) -> Vec<Inline> {
    let mut inlines = Vec::new();
    let mut cursor = 0;

    while let Some(r_start) = p_xml[cursor..].find("<w:r>") {
        let start = cursor + r_start;
        let end = match p_xml[start..].find("</w:r>") {
            Some(e) => start + e + 6,
            None => break,
        };
        cursor = end;
        let run_xml = &p_xml[start..end];

        let bold = run_xml.contains("<w:b/>") || run_xml.contains("<w:b ");
        let italic = run_xml.contains("<w:i/>") || run_xml.contains("<w:i ");
        let strike = run_xml.contains("<w:strike/>") || run_xml.contains("<w:strike ");
        let code = run_xml.contains("Consolas") || run_xml.contains("Courier");

        let text = extract_text_content(run_xml);
        if text.is_empty() {
            continue;
        }

        let mut node = if code {
            Inline::Code(text)
        } else {
            Inline::Text(text)
        };

        if bold {
            node = Inline::Strong(vec![node]);
        }
        if italic {
            node = Inline::Emph(vec![node]);
        }
        if strike {
            node = Inline::Strikethrough(vec![node]);
        }
        inlines.push(node);
    }

    inlines
}

fn extract_text_content(run_xml: &str) -> String {
    let mut text = String::new();
    let mut cursor = 0;
    while let Some(t_start) = run_xml[cursor..].find("<w:t") {
        let abs_start = cursor + t_start;
        let open_tag_end = match run_xml[abs_start..].find('>') {
            Some(gt) => abs_start + gt + 1,
            None => break,
        };
        let close_tag = match run_xml[open_tag_end..].find("</w:t>") {
            Some(ct) => open_tag_end + ct,
            None => break,
        };
        text.push_str(&run_xml[open_tag_end..close_tag]);
        cursor = close_tag + 6;
    }
    unescape_xml(&text)
}

fn unescape_xml(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
}

fn parse_table(tbl_xml: &str) -> Option<Block> {
    let mut rows_raw = Vec::new();
    let mut cursor = 0;

    while let Some(tr_start) = tbl_xml[cursor..].find("<w:tr>") {
        let start = cursor + tr_start;
        let end = match tbl_xml[start..].find("</w:tr>") {
            Some(e) => start + e + 7,
            None => break,
        };
        cursor = end;
        let tr_xml = &tbl_xml[start..end];

        let mut row_cells = Vec::new();
        let mut cell_cursor = 0;
        while let Some(tc_start) = tr_xml[cell_cursor..].find("<w:tc>") {
            let tc_abs = cell_cursor + tc_start;
            let tc_end = match tr_xml[tc_abs..].find("</w:tc>") {
                Some(e) => tc_abs + e + 7,
                None => break,
            };
            cell_cursor = tc_end;
            let inlines = parse_inlines(&tr_xml[tc_abs..tc_end]);
            row_cells.push(inlines);
        }

        if !row_cells.is_empty() {
            rows_raw.push(row_cells);
        }
    }

    if rows_raw.is_empty() {
        return None;
    }

    let header = rows_raw.remove(0);
    let cols = header.len();
    if cols == 0 {
        return None;
    }

    // Ensure all data rows have identical column length to uphold IDM invariants
    let mut valid_rows = Vec::new();
    for mut row in rows_raw {
        if row.len() > cols {
            row.truncate(cols);
        } else {
            while row.len() < cols {
                row.push(Vec::new());
            }
        }
        valid_rows.push(row);
    }

    let align = vec![Align::None; cols];
    Some(Block::Table {
        header,
        rows: valid_rows,
        align,
    })
}

// ---------------- XML Serialization (Writer) ----------------

fn serialize_document(doc: &Document) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
        <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\n  \
          <w:body>\n",
    );

    for block in &doc.blocks {
        serialize_block(&mut xml, block);
    }

    xml.push_str("    <w:sectPr/>\n  </w:body>\n</w:document>");
    xml
}

fn serialize_block(xml: &mut String, block: &Block) {
    match block {
        Block::Heading { level, inlines } => {
            xml.push_str(&format!(
                "    <w:p>\n      <w:pPr><w:pStyle w:val=\"Heading{level}\"/></w:pPr>\n"
            ));
            serialize_inlines(xml, inlines, false, false, false, false);
            xml.push_str("    </w:p>\n");
        }
        Block::Paragraph { inlines } => {
            xml.push_str("    <w:p>\n");
            serialize_inlines(xml, inlines, false, false, false, false);
            xml.push_str("    </w:p>\n");
        }
        Block::CodeBlock { text, .. } => {
            xml.push_str(
                "    <w:p>\n      <w:pPr><w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"F4F4F4\"/></w:pPr>\n\
                     <w:r><w:rPr><w:rFonts w:ascii=\"Consolas\" w:hAnsi=\"Consolas\"/></w:rPr>\
                     <w:t xml:space=\"preserve\">",
            );
            xml.push_str(&xml_escape(text));
            xml.push_str("</w:t></w:r>\n    </w:p>\n");
        }
        Block::Rule => {
            xml.push_str(
                "    <w:p>\n      <w:pPr><w:pBdr><w:bottom w:val=\"single\" w:sz=\"6\" w:space=\"1\" w:color=\"auto\"/></w:pPr></w:pPr>\n    </w:p>\n",
            );
        }
        Block::List {
            ordered,
            start,
            items,
        } => {
            let mut num = *start;
            for item in items {
                for inner_block in item {
                    match inner_block {
                        Block::Paragraph { inlines } => {
                            let prefix = if *ordered {
                                let p = format!("{num}. ");
                                num += 1;
                                p
                            } else {
                                "• ".to_string()
                            };
                            xml.push_str("    <w:p>\n      <w:r><w:t>");
                            xml.push_str(&xml_escape(&prefix));
                            xml.push_str("</w:t></w:r>\n");
                            serialize_inlines(xml, inlines, false, false, false, false);
                            xml.push_str("    </w:p>\n");
                        }
                        other => serialize_block(xml, other),
                    }
                }
            }
        }
        Block::Blockquote { blocks } => {
            for inner in blocks {
                match inner {
                    Block::Paragraph { inlines } => {
                        xml.push_str(
                            "    <w:p>\n      <w:pPr><w:pBdr><w:left w:val=\"single\" w:sz=\"18\" w:space=\"12\" w:color=\"CCCCCC\"/></w:pBdr></w:pPr>\n",
                        );
                        serialize_inlines(xml, inlines, false, false, false, false);
                        xml.push_str("    </w:p>\n");
                    }
                    other => serialize_block(xml, other),
                }
            }
        }
        Block::Table { header, rows, .. } => {
            xml.push_str(
                "    <w:tbl>\n      <w:tblPr><w:tblBorders>\
                <w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"CCCCCC\"/>\
                <w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"CCCCCC\"/>\
                <w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"CCCCCC\"/>\
                <w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"CCCCCC\"/>\
                <w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"CCCCCC\"/>\
                <w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"CCCCCC\"/>\
                </w:tblBorders></w:tblPr>\n",
            );

            // Header row
            xml.push_str("      <w:tr>\n");
            for cell in header {
                xml.push_str("        <w:tc><w:p>\n");
                serialize_inlines(xml, cell, true, false, false, false);
                xml.push_str("        </w:p></w:tc>\n");
            }
            xml.push_str("      </w:tr>\n");

            // Data rows
            for row in rows {
                xml.push_str("      <w:tr>\n");
                for cell in row {
                    xml.push_str("        <w:tc><w:p>\n");
                    serialize_inlines(xml, cell, false, false, false, false);
                    xml.push_str("        </w:p></w:tc>\n");
                }
                xml.push_str("      </w:tr>\n");
            }
            xml.push_str("    </w:tbl>\n");
        }
        Block::Image { src, alt } => {
            xml.push_str(&format!(
                "    <w:p><w:r><w:t>[Image: {} ({})]</w:t></w:r></w:p>\n",
                xml_escape(alt),
                xml_escape(src)
            ));
        }
    }
}

fn serialize_inlines(
    xml: &mut String,
    inlines: &[Inline],
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                xml.push_str("      <w:r><w:rPr>");
                if bold {
                    xml.push_str("<w:b/>");
                }
                if italic {
                    xml.push_str("<w:i/>");
                }
                if strike {
                    xml.push_str("<w:strike/>");
                }
                if code {
                    xml.push_str("<w:rFonts w:ascii=\"Consolas\" w:hAnsi=\"Consolas\"/><w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"F0F0F0\"/>");
                }
                xml.push_str("</w:rPr><w:t xml:space=\"preserve\">");
                xml.push_str(&xml_escape(t));
                xml.push_str("</w:t></w:r>\n");
            }
            Inline::Code(c) => {
                xml.push_str(
                    "      <w:r><w:rPr><w:rFonts w:ascii=\"Consolas\" w:hAnsi=\"Consolas\"/>\
                    <w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"F0F0F0\"/></w:rPr>\
                    <w:t xml:space=\"preserve\">",
                );
                xml.push_str(&xml_escape(c));
                xml.push_str("</w:t></w:r>\n");
            }
            Inline::Strong(children) => {
                serialize_inlines(xml, children, true, italic, strike, code);
            }
            Inline::Emph(children) => {
                serialize_inlines(xml, children, bold, true, strike, code);
            }
            Inline::Strikethrough(children) => {
                serialize_inlines(xml, children, bold, italic, true, code);
            }
            Inline::Link {
                inlines: children, ..
            } => {
                serialize_inlines(xml, children, bold, italic, strike, code);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docx_roundtrip_preserves_document() {
        let doc = Document {
            meta: Metadata {
                title: Some("My Document".to_string()),
                author: Some("Jane Doe".to_string()),
            },
            blocks: vec![
                Block::Heading {
                    level: 1,
                    inlines: vec![Inline::Text("Introduction".to_string())],
                },
                Block::Paragraph {
                    inlines: vec![
                        Inline::Text("Here is a ".to_string()),
                        Inline::Strong(vec![Inline::Text("bold".to_string())]),
                        Inline::Text(" and ".to_string()),
                        Inline::Emph(vec![Inline::Text("italic".to_string())]),
                        Inline::Text(" test.".to_string()),
                    ],
                },
                Block::Rule,
                Block::Table {
                    header: vec![
                        vec![Inline::Text("Col A".to_string())],
                        vec![Inline::Text("Col B".to_string())],
                    ],
                    rows: vec![vec![
                        vec![Inline::Text("Val 1".to_string())],
                        vec![Inline::Text("Val 2".to_string())],
                    ]],
                    align: vec![Align::None, Align::None],
                },
            ],
        };

        let writer = DocxWriter;
        let bytes = writer.write(&doc, Format::Docx).expect("docx written");
        assert!(!bytes.is_empty());

        let reader = DocxReader;
        let parsed = reader.read(&bytes, Format::Docx).expect("docx read");

        assert_eq!(parsed.meta.title.as_deref(), Some("My Document"));
        assert_eq!(parsed.meta.author.as_deref(), Some("Jane Doe"));
        assert_eq!(parsed.blocks.len(), 4);
        assert!(matches!(parsed.blocks[0], Block::Heading { level: 1, .. }));
        assert!(matches!(parsed.blocks[1], Block::Paragraph { .. }));
        assert!(matches!(parsed.blocks[2], Block::Rule));
        assert!(matches!(parsed.blocks[3], Block::Table { .. }));
    }
}
