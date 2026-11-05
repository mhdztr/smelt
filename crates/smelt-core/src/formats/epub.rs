//! EPUB converter: zip + XHTML spine; reuses the HTML converter.

use std::io::{Cursor, Read, Write};

use smelt_detect::Format;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::error::{ReadError, WriteError};
use crate::idm::{Document, Metadata};
use crate::{Reader, Writer};

use super::html::{HtmlReader, HtmlWriter};

/// The epub reader.
pub struct EpubReader;

/// The epub writer.
pub struct EpubWriter;

impl Reader for EpubReader {
    fn read(&self, input: &[u8], _fmt: Format) -> Result<Document, ReadError> {
        let cursor = Cursor::new(input);
        let mut archive = ZipArchive::new(cursor).map_err(|e| ReadError::Parse {
            format: "epub".to_string(),
            message: format!("invalid epub zip: {e}"),
        })?;

        let mut meta = Metadata::default();
        // Try to extract metadata from OPF if present
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| ReadError::Parse {
                format: "epub".to_string(),
                message: format!("epub read error: {e}"),
            })?;
            let name = file.name().to_string();
            if name.ends_with(".opf") {
                let mut contents = String::new();
                file.read_to_string(&mut contents).ok();
                if let Some(title) = extract_tag(&contents, "dc:title") {
                    meta.title = Some(title);
                }
                if let Some(author) = extract_tag(&contents, "dc:creator") {
                    meta.author = Some(author);
                }
                break;
            }
        }

        // Collect HTML/XHTML files from spine - look for common patterns
        let mut html_parts: Vec<String> = Vec::new();
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| ReadError::Parse {
                format: "epub".to_string(),
                message: format!("epub read error: {e}"),
            })?;
            let name = file.name().to_string();
            if name.starts_with("META-INF/") {
                continue;
            }
            if name == "mimetype" || name.ends_with(".opf") || name.ends_with(".ncx") {
                continue;
            }
            if name.ends_with(".xhtml") || name.ends_with(".html") || name.ends_with(".htm") {
                let mut contents = String::new();
                if file.read_to_string(&mut contents).is_ok() {
                    html_parts.push(contents);
                }
            }
        }

        if html_parts.is_empty() {
            return Err(ReadError::Parse {
                format: "epub".to_string(),
                message: "no readable content in epub".to_string(),
            });
        }

        let combined = html_parts.join("\n");
        // Parse as HTML fragment/document
        let html_doc = HtmlReader.read(combined.as_bytes(), Format::Html)?;
        Ok(Document {
            meta,
            blocks: html_doc.blocks,
        })
    }
}

fn extract_tag(s: &str, tag: &str) -> Option<String> {
    // simple extraction
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);
    if let Some(start) = s.find(&start_tag) {
        let rest = &s[start + start_tag.len()..];
        if let Some(end) = rest.find(&end_tag) {
            let content = rest[..end].trim();
            // strip inner tags if any
            let content = strip_tags(content);
            if !content.is_empty() {
                return Some(content.to_string());
            }
        }
    }
    // try with namespace
    None
}

fn strip_tags(s: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        if c == '<' {
            in_tag = true;
            continue;
        }
        if c == '>' && in_tag {
            in_tag = false;
            continue;
        }
        if !in_tag {
            result.push(c);
        }
    }
    result.trim().to_string()
}

impl Writer for EpubWriter {
    fn write(&self, doc: &Document, _fmt: Format) -> Result<Vec<u8>, WriteError> {
        let mut buffer = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(&mut buffer);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .unix_permissions(0o644);

        // mimetype must be first and uncompressed
        zip.start_file("mimetype", options)
            .map_err(|e| WriteError::Serialize {
                format: "epub".to_string(),
                message: format!("zip error: {e}"),
            })?;
        zip.write_all(b"application/epub+zip")
            .map_err(|e| WriteError::Serialize {
                format: "epub".to_string(),
                message: format!("zip error: {e}"),
            })?;

        let options_deflated = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        zip.start_file("META-INF/container.xml", options_deflated)
            .map_err(|e| WriteError::Serialize {
                format: "epub".to_string(),
                message: format!("zip error: {e}"),
            })?;
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>
"#,
        )
        .map_err(|e| WriteError::Serialize {
            format: "epub".to_string(),
            message: format!("zip error: {e}"),
        })?;

        // Convert document to HTML
        let html_bytes = HtmlWriter.write(doc, Format::Html)?;
        let html = String::from_utf8(html_bytes).map_err(|_| WriteError::Serialize {
            format: "epub".to_string(),
            message: "invalid utf8".to_string(),
        })?;

        zip.start_file("OEBPS/content.xhtml", options_deflated)
            .map_err(|e| WriteError::Serialize {
                format: "epub".to_string(),
                message: format!("zip error: {e}"),
            })?;
        zip.write_all(html.as_bytes())
            .map_err(|e| WriteError::Serialize {
                format: "epub".to_string(),
                message: format!("zip error: {e}"),
            })?;

        let title = doc.meta.title.as_deref().unwrap_or("Untitled");
        let author = doc.meta.author.as_deref().unwrap_or("Unknown");
        let opf = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookId" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>{}</dc:title>
    <dc:creator>{}</dc:creator>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="content" href="content.xhtml" media-type="application/xhtml+xml"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
  </manifest>
  <spine toc="ncx">
    <itemref idref="content"/>
  </spine>
</package>
"#,
            esc_xml(title),
            esc_xml(author)
        );
        zip.start_file("OEBPS/content.opf", options_deflated)
            .map_err(|e| WriteError::Serialize {
                format: "epub".to_string(),
                message: format!("zip error: {e}"),
            })?;
        zip.write_all(opf.as_bytes())
            .map_err(|e| WriteError::Serialize {
                format: "epub".to_string(),
                message: format!("zip error: {e}"),
            })?;

        let ncx = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head>
    <meta name="dtb:uid" content="bookid"/>
  </head>
  <docTitle><text>{}</text></docTitle>
  <navMap>
    <navPoint id="navPoint-1" playOrder="1">
      <navLabel><text>{}</text></navLabel>
      <content src="content.xhtml"/>
    </navPoint>
  </navMap>
</ncx>
"#,
            esc_xml(title),
            esc_xml(title)
        );
        zip.start_file("OEBPS/toc.ncx", options_deflated)
            .map_err(|e| WriteError::Serialize {
                format: "epub".to_string(),
                message: format!("zip error: {e}"),
            })?;
        zip.write_all(ncx.as_bytes())
            .map_err(|e| WriteError::Serialize {
                format: "epub".to_string(),
                message: format!("zip error: {e}"),
            })?;

        zip.finish().map_err(|e| WriteError::Serialize {
            format: "epub".to_string(),
            message: format!("zip error: {e}"),
        })?;
        Ok(buffer.into_inner())
    }
}

fn esc_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
