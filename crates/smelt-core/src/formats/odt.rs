//! Native ODT (OpenDocument Text) converter: reads and writes content.xml in ODF ZIP container.
//! Zero external runtime required (LibreOffice not needed).

use std::io::{Cursor, Read, Write};

use smelt_detect::Format;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::error::{ReadError, WriteError};
use crate::idm::{Align, Block, Document, Inline, Metadata};
use crate::{Reader, Writer};

/// The native ODT reader.
pub struct OdtReader;

/// The native ODT writer.
pub struct OdtWriter;

impl Reader for OdtReader {
    fn read(&self, input: &[u8], _fmt: Format) -> Result<Document, ReadError> {
        let cursor = Cursor::new(input);
        let mut archive = ZipArchive::new(cursor).map_err(|e| ReadError::Parse {
            format: "odt".to_string(),
            message: format!("invalid odt archive: {e}"),
        })?;

        let mut meta = Metadata::default();

        // 1. Try reading meta.xml for title and author
        if let Ok(mut meta_file) = archive.by_name("meta.xml") {
            let mut meta_xml = String::new();
            if meta_file.read_to_string(&mut meta_xml).is_ok() {
                if let Some(title) = extract_tag(&meta_xml, "dc:title")
                    && !title.is_empty()
                {
                    meta.title = Some(title);
                }
                if let Some(author) = extract_tag(&meta_xml, "dc:creator")
                    && !author.is_empty()
                {
                    meta.author = Some(author);
                }
            }
        }

        // 2. Read content.xml
        let mut content_file = archive
            .by_name("content.xml")
            .map_err(|e| ReadError::Parse {
                format: "odt".to_string(),
                message: format!("content.xml missing: {e}"),
            })?;

        let mut content_xml = String::new();
        content_file
            .read_to_string(&mut content_xml)
            .map_err(|e| ReadError::Parse {
                format: "odt".to_string(),
                message: format!("failed to read content.xml: {e}"),
            })?;

        let blocks = parse_odt_body(&content_xml);

        Ok(Document { meta, blocks })
    }
}

fn parse_odt_body(xml: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let body_start = match xml.find("<office:text") {
        Some(pos) => match xml[pos..].find('>') {
            Some(end) => pos + end + 1,
            None => 0,
        },
        None => 0,
    };
    let body_end = xml.rfind("</office:text>").unwrap_or(xml.len());
    let body_xml = if body_start < body_end {
        &xml[body_start..body_end]
    } else {
        xml
    };

    let mut cursor = 0;
    while cursor < body_xml.len() {
        let next_tag_start = match body_xml[cursor..].find('<') {
            Some(idx) => cursor + idx,
            None => break,
        };

        let tag_open_end = match body_xml[next_tag_start..].find('>') {
            Some(idx) => next_tag_start + idx + 1,
            None => break,
        };
        let full_tag = &body_xml[next_tag_start..tag_open_end];

        if full_tag.starts_with("<text:h") {
            let end_tag = "</text:h>";
            if let Some(close_idx) = body_xml[tag_open_end..].find(end_tag) {
                let inner_end = tag_open_end + close_idx;
                let level = extract_attr(full_tag, "text:outline-level")
                    .and_then(|s| s.parse::<u8>().ok())
                    .unwrap_or(1)
                    .clamp(1, 6);
                let content = &body_xml[tag_open_end..inner_end];
                let inlines = parse_odt_inlines(content);
                if !inlines.is_empty() {
                    blocks.push(Block::Heading { level, inlines });
                }
                cursor = inner_end + end_tag.len();
                continue;
            }
        } else if full_tag.starts_with("<text:p") {
            let end_tag = "</text:p>";
            if let Some(close_idx) = body_xml[tag_open_end..].find(end_tag) {
                let inner_end = tag_open_end + close_idx;
                let content = &body_xml[tag_open_end..inner_end];
                let inlines = parse_odt_inlines(content);
                if !inlines.is_empty() {
                    blocks.push(Block::Paragraph { inlines });
                }
                cursor = inner_end + end_tag.len();
                continue;
            }
        } else if full_tag.starts_with("<table:table") {
            let end_tag = "</table:table>";
            if let Some(close_idx) = body_xml[tag_open_end..].find(end_tag) {
                let inner_end = tag_open_end + close_idx;
                let table_content = &body_xml[tag_open_end..inner_end];
                if let Some(table_block) = parse_odt_table(table_content) {
                    blocks.push(table_block);
                }
                cursor = inner_end + end_tag.len();
                continue;
            }
        } else if full_tag.starts_with("<text:list") {
            let end_tag = "</text:list>";
            if let Some(close_idx) = body_xml[tag_open_end..].find(end_tag) {
                let inner_end = tag_open_end + close_idx;
                let list_content = &body_xml[tag_open_end..inner_end];
                let items = parse_odt_list(list_content);
                if !items.is_empty() {
                    blocks.push(Block::List {
                        ordered: false,
                        start: 1,
                        items,
                    });
                }
                cursor = inner_end + end_tag.len();
                continue;
            }
        }

        cursor = next_tag_start + 1;
    }

    blocks
}

fn parse_odt_table(xml: &str) -> Option<Block> {
    let mut rows: Vec<Vec<Vec<Inline>>> = Vec::new();
    let mut cursor = 0;

    while let Some(row_start) = xml[cursor..].find("<table:table-row") {
        let abs_start = cursor + row_start;
        let tag_end = match xml[abs_start..].find('>') {
            Some(idx) => abs_start + idx + 1,
            None => break,
        };
        let row_close = match xml[tag_end..].find("</table:table-row>") {
            Some(idx) => tag_end + idx,
            None => break,
        };
        let row_xml = &xml[tag_end..row_close];
        cursor = row_close + "</table:table-row>".len();

        let mut cells = Vec::new();
        let mut cell_cursor = 0;
        while let Some(cell_start) = row_xml[cell_cursor..].find("<table:table-cell") {
            let abs_cell = cell_cursor + cell_start;
            let cell_tag_end = match row_xml[abs_cell..].find('>') {
                Some(idx) => abs_cell + idx + 1,
                None => break,
            };
            let cell_close = match row_xml[cell_tag_end..].find("</table:table-cell>") {
                Some(idx) => cell_tag_end + idx,
                None => break,
            };
            let cell_xml = &row_xml[cell_tag_end..cell_close];
            cell_cursor = cell_close + "</table:table-cell>".len();

            let inlines = parse_odt_inlines(cell_xml);
            cells.push(inlines);
        }

        if !cells.is_empty() {
            rows.push(cells);
        }
    }

    if rows.is_empty() {
        return None;
    }

    let header = rows.remove(0);
    let cols = header.len();
    let mut valid_rows = Vec::new();
    for mut r in rows {
        if r.len() == cols {
            valid_rows.push(r);
        } else if r.len() < cols {
            while r.len() < cols {
                r.push(vec![]);
            }
            valid_rows.push(r);
        } else {
            r.truncate(cols);
            valid_rows.push(r);
        }
    }

    Some(Block::Table {
        header,
        rows: valid_rows,
        align: vec![Align::None; cols],
    })
}

fn parse_odt_list(xml: &str) -> Vec<Vec<Block>> {
    let mut items = Vec::new();
    let mut cursor = 0;

    while let Some(item_start) = xml[cursor..].find("<text:list-item") {
        let abs_start = cursor + item_start;
        let tag_end = match xml[abs_start..].find('>') {
            Some(idx) => abs_start + idx + 1,
            None => break,
        };
        let item_close = match xml[tag_end..].find("</text:list-item>") {
            Some(idx) => tag_end + idx,
            None => break,
        };
        let item_xml = &xml[tag_end..item_close];
        cursor = item_close + "</text:list-item>".len();

        let inlines = parse_odt_inlines(item_xml);
        if !inlines.is_empty() {
            items.push(vec![Block::Paragraph { inlines }]);
        }
    }

    items
}

fn parse_odt_inlines(xml: &str) -> Vec<Inline> {
    let mut inlines = Vec::new();
    let mut cursor = 0;

    while cursor < xml.len() {
        if let Some(tag_pos) = xml[cursor..].find('<') {
            let text_before = &xml[cursor..cursor + tag_pos];
            if !text_before.is_empty() {
                inlines.push(Inline::Text(decode_xml_entities(text_before)));
            }

            let abs_tag = cursor + tag_pos;
            if let Some(tag_end) = xml[abs_tag..].find('>') {
                let full_tag = &xml[abs_tag..=abs_tag + tag_end];
                cursor = abs_tag + tag_end + 1;

                if full_tag.starts_with("<text:span") {
                    let style = extract_attr(full_tag, "text:style-name").unwrap_or_default();
                    let lower = style.to_lowercase();
                    let is_bold = lower.contains("bold");
                    let is_italic = lower.contains("italic") || lower.contains("oblique");

                    if let Some(close_pos) = xml[cursor..].find("</text:span>") {
                        let span_content = &xml[cursor..cursor + close_pos];
                        cursor += close_pos + "</text:span>".len();

                        let mut inner = parse_odt_inlines(span_content);
                        if is_italic {
                            inner = vec![Inline::Emph(inner)];
                        }
                        if is_bold {
                            inner = vec![Inline::Strong(inner)];
                        }
                        inlines.extend(inner);
                    }
                } else if full_tag.starts_with("<text:s ")
                    || full_tag == "<text:s/>"
                    || full_tag == "<text:s>"
                {
                    let count = extract_attr(full_tag, "text:c")
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(1);
                    inlines.push(Inline::Text(" ".repeat(count)));
                } else if full_tag.starts_with("<text:tab") {
                    inlines.push(Inline::Text("\t".to_string()));
                } else if full_tag.starts_with("<text:line-break") {
                    inlines.push(Inline::Text("\n".to_string()));
                }
            } else {
                break;
            }
        } else {
            let remaining = &xml[cursor..];
            if !remaining.is_empty() {
                inlines.push(Inline::Text(decode_xml_entities(remaining)));
            }
            break;
        }
    }

    normalize_inlines(inlines)
}

fn normalize_inlines(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                if t.is_empty() {
                    continue;
                }
                if let Some(Inline::Text(prev)) = out.last_mut() {
                    prev.push_str(&t);
                } else {
                    out.push(Inline::Text(t));
                }
            }
            Inline::Strong(mut inner) => {
                let norm = normalize_inlines(std::mem::take(&mut inner));
                if !norm.is_empty() {
                    out.push(Inline::Strong(norm));
                }
            }
            Inline::Emph(mut inner) => {
                let norm = normalize_inlines(std::mem::take(&mut inner));
                if !norm.is_empty() {
                    out.push(Inline::Emph(norm));
                }
            }
            Inline::Strikethrough(mut inner) => {
                let norm = normalize_inlines(std::mem::take(&mut inner));
                if !norm.is_empty() {
                    out.push(Inline::Strikethrough(norm));
                }
            }
            other => out.push(other),
        }
    }
    out
}

impl Writer for OdtWriter {
    fn write(&self, doc: &Document, _fmt: Format) -> Result<Vec<u8>, WriteError> {
        let buffer = Vec::new();
        let cursor = Cursor::new(buffer);
        let mut zip = ZipWriter::new(cursor);

        let stored_options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflated_options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        // 1. mimetype (MUST be first and uncompressed)
        zip.start_file("mimetype", stored_options)
            .map_err(|e| WriteError::Serialize {
                format: "odt".to_string(),
                message: format!("zip error: {e}"),
            })?;
        zip.write_all(b"application/vnd.oasis.opendocument.text")
            .map_err(|e| WriteError::Serialize {
                format: "odt".to_string(),
                message: format!("write error: {e}"),
            })?;

        // 2. META-INF/manifest.xml
        zip.start_file("META-INF/manifest.xml", deflated_options)
            .map_err(|e| WriteError::Serialize {
                format: "odt".to_string(),
                message: format!("zip error: {e}"),
            })?;
        zip.write_all(MANIFEST_XML.as_bytes())
            .map_err(|e| WriteError::Serialize {
                format: "odt".to_string(),
                message: format!("write error: {e}"),
            })?;

        // 3. meta.xml
        let meta_xml = build_meta_xml(doc);
        zip.start_file("meta.xml", deflated_options)
            .map_err(|e| WriteError::Serialize {
                format: "odt".to_string(),
                message: format!("zip error: {e}"),
            })?;
        zip.write_all(meta_xml.as_bytes())
            .map_err(|e| WriteError::Serialize {
                format: "odt".to_string(),
                message: format!("write error: {e}"),
            })?;

        // 4. content.xml
        let content_xml = build_content_xml(doc);
        zip.start_file("content.xml", deflated_options)
            .map_err(|e| WriteError::Serialize {
                format: "odt".to_string(),
                message: format!("zip error: {e}"),
            })?;
        zip.write_all(content_xml.as_bytes())
            .map_err(|e| WriteError::Serialize {
                format: "odt".to_string(),
                message: format!("write error: {e}"),
            })?;

        let finished = zip.finish().map_err(|e| WriteError::Serialize {
            format: "odt".to_string(),
            message: format!("zip finalize error: {e}"),
        })?;

        Ok(finished.into_inner())
    }
}

const MANIFEST_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">
 <manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text"/>
 <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
 <manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/>
</manifest:manifest>"#;

fn build_meta_xml(doc: &Document) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-meta xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n<office:meta>\n",
    );
    if let Some(ref title) = doc.meta.title {
        xml.push_str("<dc:title>");
        escape_xml_to(&mut xml, title);
        xml.push_str("</dc:title>\n");
    }
    if let Some(ref author) = doc.meta.author {
        xml.push_str("<dc:creator>");
        escape_xml_to(&mut xml, author);
        xml.push_str("</dc:creator>\n");
    }
    xml.push_str("</office:meta>\n</office:document-meta>");
    xml
}

fn build_content_xml(doc: &Document) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
 xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
 xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"
 xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
 xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0">
<office:automatic-styles>
 <style:style style:name="Bold" style:family="text">
  <style:text-properties fo:font-weight="bold"/>
 </style:style>
 <style:style style:name="Italic" style:family="text">
  <style:text-properties fo:font-style="italic"/>
 </style:style>
 <style:style style:name="BoldItalic" style:family="text">
  <style:text-properties fo:font-weight="bold" fo:font-style="italic"/>
 </style:style>
</office:automatic-styles>
<office:body>
<office:text>
"#,
    );

    for block in &doc.blocks {
        match block {
            Block::Heading { level, inlines } => {
                xml.push_str(&format!("<text:h text:outline-level=\"{level}\">"));
                serialize_inlines(&mut xml, inlines, false, false);
                xml.push_str("</text:h>\n");
            }
            Block::Paragraph { inlines } => {
                xml.push_str("<text:p>");
                serialize_inlines(&mut xml, inlines, false, false);
                xml.push_str("</text:p>\n");
            }
            Block::CodeBlock { text, .. } => {
                for line in text.lines() {
                    xml.push_str("<text:p>");
                    escape_xml_to(&mut xml, line);
                    xml.push_str("</text:p>\n");
                }
            }
            Block::List { items, .. } => {
                xml.push_str("<text:list>\n");
                for item in items {
                    xml.push_str("<text:list-item>\n");
                    for b in item {
                        if let Block::Paragraph { inlines } = b {
                            xml.push_str("<text:p>");
                            serialize_inlines(&mut xml, inlines, false, false);
                            xml.push_str("</text:p>\n");
                        }
                    }
                    xml.push_str("</text:list-item>\n");
                }
                xml.push_str("</text:list>\n");
            }
            Block::Table { header, rows, .. } => {
                xml.push_str("<table:table>\n");
                // Header row
                xml.push_str("<table:table-row>\n");
                for cell in header {
                    xml.push_str("<table:table-cell><text:p>");
                    serialize_inlines(&mut xml, cell, true, false);
                    xml.push_str("</text:p></table:table-cell>\n");
                }
                xml.push_str("</table:table-row>\n");
                // Body rows
                for row in rows {
                    xml.push_str("<table:table-row>\n");
                    for cell in row {
                        xml.push_str("<table:table-cell><text:p>");
                        serialize_inlines(&mut xml, cell, false, false);
                        xml.push_str("</text:p></table:table-cell>\n");
                    }
                    xml.push_str("</table:table-row>\n");
                }
                xml.push_str("</table:table-row>\n");
                xml.push_str("</table:table>\n");
            }
            Block::Blockquote { blocks } => {
                for b in blocks {
                    if let Block::Paragraph { inlines } = b {
                        xml.push_str("<text:p>");
                        serialize_inlines(&mut xml, inlines, false, true);
                        xml.push_str("</text:p>\n");
                    }
                }
            }
            Block::Image { alt, .. } => {
                xml.push_str("<text:p>[Image: ");
                escape_xml_to(&mut xml, alt);
                xml.push_str("]</text:p>\n");
            }
            Block::Rule => {
                xml.push_str("<text:p>----------------------------------------</text:p>\n");
            }
        }
    }

    xml.push_str("</office:text>\n</office:body>\n</office:document-content>");
    xml
}

fn serialize_inlines(xml: &mut String, inlines: &[Inline], bold: bool, italic: bool) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                let style = match (bold, italic) {
                    (true, true) => Some("BoldItalic"),
                    (true, false) => Some("Bold"),
                    (false, true) => Some("Italic"),
                    (false, false) => None,
                };
                if let Some(s) = style {
                    xml.push_str(&format!("<text:span text:style-name=\"{s}\">"));
                    escape_xml_to(xml, t);
                    xml.push_str("</text:span>");
                } else {
                    escape_xml_to(xml, t);
                }
            }
            Inline::Code(c) => {
                escape_xml_to(xml, c);
            }
            Inline::Strong(children) => {
                serialize_inlines(xml, children, true, italic);
            }
            Inline::Emph(children) => {
                serialize_inlines(xml, children, bold, true);
            }
            Inline::Strikethrough(children) => {
                serialize_inlines(xml, children, bold, italic);
            }
            Inline::Link {
                inlines: children, ..
            } => {
                serialize_inlines(xml, children, bold, italic);
            }
        }
    }
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open_tag = format!("<{tag}>");
    let close_tag = format!("</{tag}>");
    let start = xml.find(&open_tag)? + open_tag.len();
    let end = xml[start..].find(&close_tag)? + start;
    Some(decode_xml_entities(&xml[start..end]))
}

fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

fn escape_xml_to(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
}

fn decode_xml_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odt_roundtrip_preserves_document() {
        let doc = Document {
            meta: Metadata {
                title: Some("ODT Title".to_string()),
                author: Some("Jane Author".to_string()),
            },
            blocks: vec![
                Block::Heading {
                    level: 1,
                    inlines: vec![Inline::Text("Hello ODT".to_string())],
                },
                Block::Paragraph {
                    inlines: vec![
                        Inline::Text("Normal and ".to_string()),
                        Inline::Strong(vec![Inline::Text("bold".to_string())]),
                    ],
                },
                Block::Table {
                    header: vec![
                        vec![Inline::Text("Col1".to_string())],
                        vec![Inline::Text("Col2".to_string())],
                    ],
                    rows: vec![vec![
                        vec![Inline::Text("A".to_string())],
                        vec![Inline::Text("B".to_string())],
                    ]],
                    align: vec![Align::None, Align::None],
                },
            ],
        };

        let bytes = OdtWriter.write(&doc, Format::Odt).expect("write odt");
        assert!(bytes.starts_with(b"PK"));

        let read_doc = OdtReader.read(&bytes, Format::Odt).expect("read odt");
        assert_eq!(read_doc.meta.title.as_deref(), Some("ODT Title"));
        assert_eq!(read_doc.meta.author.as_deref(), Some("Jane Author"));
        assert_eq!(read_doc.blocks.len(), 3);

        match &read_doc.blocks[0] {
            Block::Heading { level, inlines } => {
                assert_eq!(*level, 1);
                assert_eq!(inlines, &vec![Inline::Text("Hello ODT".to_string())]);
            }
            other => panic!("expected Heading, got {other:?}"),
        }

        match &read_doc.blocks[1] {
            Block::Paragraph { inlines } => {
                assert_eq!(
                    inlines,
                    &vec![
                        Inline::Text("Normal and ".to_string()),
                        Inline::Strong(vec![Inline::Text("bold".to_string())]),
                    ]
                );
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }

        match &read_doc.blocks[2] {
            Block::Table { header, rows, .. } => {
                assert_eq!(header.len(), 2);
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], vec![Inline::Text("A".to_string())]);
            }
            other => panic!("expected Table, got {other:?}"),
        }
    }
}
