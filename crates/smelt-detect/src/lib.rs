//! Magic-byte document format detection for smelt.
//!
//! This crate is a deliberate leaf: zero dependencies, no I/O beyond a small
//! prefix read in `detect_path`. Detection order is magic bytes first, file
//! extension only as tiebreak/disambiguator; see `FORMATS.md` for the table
//! and `docs/cli.md` for how the CLI reports the result.

use std::path::Path;

/// A document format smelt knows about (detected or targeted).
///
/// Detection covers every variant; conversion support lags behind
/// (`FORMATS.md` marks which milestone converts each format natively).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Markdown,
    Html,
    Text,
    Pdf,
    Docx,
    Odt,
    Xlsx,
    Pptx,
    Epub,
    Rtf,
    Csv,
    Tsv,
    Unknown,
}

impl Format {
    /// Canonical display name (stable spelling used by `info` and errors).
    pub fn name(self) -> &'static str {
        match self {
            Format::Markdown => "Markdown",
            Format::Html => "Html",
            Format::Text => "Text",
            Format::Pdf => "Pdf",
            Format::Docx => "Docx",
            Format::Odt => "Odt",
            Format::Xlsx => "Xlsx",
            Format::Pptx => "Pptx",
            Format::Epub => "Epub",
            Format::Rtf => "Rtf",
            Format::Csv => "Csv",
            Format::Tsv => "Tsv",
            Format::Unknown => "Unknown",
        }
    }

    /// Canonical file extension (no dot).
    pub fn ext(self) -> &'static str {
        match self {
            Format::Markdown => "md",
            Format::Html => "html",
            Format::Text => "txt",
            Format::Pdf => "pdf",
            Format::Docx => "docx",
            Format::Odt => "odt",
            Format::Xlsx => "xlsx",
            Format::Pptx => "pptx",
            Format::Epub => "epub",
            Format::Rtf => "rtf",
            Format::Csv => "csv",
            Format::Tsv => "tsv",
            Format::Unknown => "txt",
        }
    }

    /// Parse a user-supplied format name (`-t md`, `--to html`, ...).
    ///
    /// Accepts the canonical extension and common aliases, case-insensitive.
    /// `Unknown` is never returned by a successful parse; `None` means the
    /// name is not a known format.
    pub fn from_name(name: &str) -> Option<Format> {
        let n = name.trim().trim_start_matches('.').to_ascii_lowercase();
        Some(match n.as_str() {
            "md" | "markdown" | "mdown" | "mkd" => Format::Markdown,
            "html" | "htm" | "xhtml" => Format::Html,
            "txt" | "text" | "plain" => Format::Text,
            "pdf" => Format::Pdf,
            "docx" => Format::Docx,
            "odt" => Format::Odt,
            "xlsx" => Format::Xlsx,
            "pptx" => Format::Pptx,
            "epub" => Format::Epub,
            "rtf" => Format::Rtf,
            "csv" => Format::Csv,
            "tsv" => Format::Tsv,
            _ => return None,
        })
    }

    /// Formats smelt can parse natively in v0 (pure Rust, no external tool).
    pub fn has_native_reader(self) -> bool {
        matches!(self, Format::Markdown | Format::Html | Format::Text)
    }

    /// Formats smelt can write natively in v0.
    pub fn has_native_writer(self) -> bool {
        matches!(self, Format::Markdown | Format::Html | Format::Text)
    }
}

/// How sure detection is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// A magic byte or unambiguous archive marker matched.
    Certain,
    /// Only heuristics (extension, loose tag scan) matched.
    Guessed,
}

/// The result of detecting a format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detection {
    pub format: Format,
    pub confidence: Confidence,
    /// Short, human-readable reason (shown by `smelt info` and `-v`).
    pub evidence: &'static str,
}

/// Bytes read from the head of a file for detection.
pub const PREFIX_LEN: usize = 1024 * 1024;

/// Detect the format of an in-memory prefix.
///
/// `ext` is the file extension without the dot (any case), used only as a
/// tiebreak when magic bytes do not decide. Detection order:
///
/// 1. `%PDF-` -> PDF
/// 2. `PK\x03\x04` -> zip family, disambiguated by stored entry names
/// 3. `{\rtf` -> RTF
/// 4. HTML tag scan (doctype certain, bare tags guessed)
/// 5. valid UTF-8 -> text family, split by extension
/// 6. otherwise -> Unknown
pub fn detect(bytes: &[u8], ext: Option<&str>) -> Detection {
    if bytes.starts_with(b"%PDF-") {
        return Detection {
            format: Format::Pdf,
            confidence: Confidence::Certain,
            evidence: "magic %PDF-",
        };
    }

    if bytes.starts_with(b"PK\x03\x04") {
        return detect_zip(bytes);
    }

    if let Some(d) = detect_rtf(bytes) {
        return d;
    }

    if let Some(d) = detect_html(bytes) {
        return d;
    }

    if valid_utf8(bytes) {
        return detect_text_family(ext);
    }

    Detection {
        format: Format::Unknown,
        confidence: Confidence::Guessed,
        evidence: "no known magic bytes, not valid UTF-8",
    }
}

/// Detect the format of a file on disk, reading at most [`PREFIX_LEN`] bytes
/// of its head. The extension comes from the path itself.
pub fn detect_path(path: &Path) -> std::io::Result<Detection> {
    use std::io::Read;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());

    let mut file = std::fs::File::open(path)?;
    let mut buf = Vec::new();
    let mut take = (&mut file).take(PREFIX_LEN as u64);
    take.read_to_end(&mut buf)?;
    Ok(detect(&buf, ext.as_deref()))
}

fn detect_zip(bytes: &[u8]) -> Detection {
    // Every Office/OpenDocument/EPUB format is a zip; the stored entry names
    // appear uncompressed in local headers, so a raw substring scan over the
    // prefix is robust.
    let has = |needle: &str| bytes.windows(needle.len()).any(|w| w == needle.as_bytes());

    if has("mimetypeapplication/epub+zip") {
        return Detection {
            format: Format::Epub,
            confidence: Confidence::Certain,
            evidence: "zip marker: stored epub mimetype entry",
        };
    }
    if has("word/document.xml") {
        return Detection {
            format: Format::Docx,
            confidence: Confidence::Certain,
            evidence: "zip marker: word/document.xml",
        };
    }
    if has("ppt/presentation.xml") {
        return Detection {
            format: Format::Pptx,
            confidence: Confidence::Certain,
            evidence: "zip marker: ppt/presentation.xml",
        };
    }
    if has("xl/workbook.xml") {
        return Detection {
            format: Format::Xlsx,
            confidence: Confidence::Certain,
            evidence: "zip marker: xl/workbook.xml",
        };
    }
    if has("mimetype") && has("content.xml") {
        return Detection {
            format: Format::Odt,
            confidence: Confidence::Certain,
            evidence: "zip marker: mimetype + content.xml",
        };
    }
    Detection {
        format: Format::Unknown,
        confidence: Confidence::Guessed,
        evidence: "zip archive (unrecognized layout)",
    }
}

fn detect_rtf(bytes: &[u8]) -> Option<Detection> {
    // RTF must start at byte 0, but tolerate up to 32 bytes of BOM/whitespace.
    let start = skip_leading_noise(bytes, 32);
    let rest = bytes.get(start..)?;
    if rest.starts_with(b"{\\rtf") {
        return Some(Detection {
            format: Format::Rtf,
            confidence: Confidence::Certain,
            evidence: "magic {\\rtf",
        });
    }
    None
}

fn detect_html(bytes: &[u8]) -> Option<Detection> {
    let start = skip_bom_ws(bytes);
    let rest = bytes.get(start..)?;
    // Skip an XML declaration ("<?xml ... ?>") if present (XHTML, EPUB spine).
    let rest: &[u8] = if rest.starts_with(b"<?xml") {
        let end = rest.windows(2).position(|w| w == b"?>")? + 2;
        rest.get(end..)?
    } else {
        rest
    };
    let rest = rest.get(skip_bom_ws(rest)..)?;

    // Scan the first KiB case-insensitively for structural HTML tags.
    let scan_len = rest.len().min(1024);
    let head = &rest[..scan_len];
    let lower: Vec<u8> = head.iter().map(|b| b.to_ascii_lowercase()).collect();

    if find(&lower, b"<!doctype html").is_some() {
        return Some(Detection {
            format: Format::Html,
            confidence: Confidence::Certain,
            evidence: "tag scan: <!DOCTYPE html>",
        });
    }
    for marker in [&b"<html"[..], &b"<head"[..], &b"<body"[..]] {
        if find(&lower, marker).is_some() {
            return Some(Detection {
                format: Format::Html,
                confidence: Confidence::Guessed,
                evidence: "tag scan: structural HTML tag",
            });
        }
    }
    None
}

fn detect_text_family(ext: Option<&str>) -> Detection {
    let ext: &str = &ext.unwrap_or("").to_ascii_lowercase();
    let (format, evidence) = match ext {
        "md" | "markdown" | "mdown" => (Format::Markdown, "extension .md"),
        "txt" | "text" | "" => (Format::Text, "valid UTF-8 text"),
        "csv" => (Format::Csv, "extension .csv"),
        "tsv" => (Format::Tsv, "extension .tsv"),
        _ => (Format::Text, "valid UTF-8 text"),
    };
    Detection {
        format,
        confidence: Confidence::Guessed,
        evidence,
    }
}

/// Whether the bytes decode as UTF-8. A multi-byte char cut off at the very
/// end of the prefix (we read a bounded window) does not count as a failure.
fn valid_utf8(bytes: &[u8]) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none(),
    }
}

/// Index of the first byte after a UTF-8 BOM plus ASCII whitespace.
fn skip_bom_ws(bytes: &[u8]) -> usize {
    let mut i = 0;
    if bytes.starts_with("\u{feff}".as_bytes()) {
        i = 3;
    }
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    i
}

/// Index of the first byte after at most `max` bytes of leading
/// whitespace/BOM (RTF allows no leading content, but real files drift).
fn skip_leading_noise(bytes: &[u8], max: usize) -> usize {
    let limit = bytes.len().min(max);
    let mut i = 0;
    if bytes[..limit].starts_with("\u{feff}".as_bytes()) {
        i = 3.min(limit);
    }
    while i < limit && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    i
}

/// Naive substring search (fine at prefix scale; zero deps).
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal zip local-file-header stream with a stored (no
    /// compression) entry. Test-only; content is enough for our scanners.
    fn stored_zip_entry(name: &str, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        out.extend_from_slice(&0u16.to_le_bytes()); // mod time
        out.extend_from_slice(&0u16.to_le_bytes()); // mod date
        out.extend_from_slice(&0u32.to_le_bytes()); // crc32 (unchecked here)
        out.extend_from_slice(&(data.len() as u32).to_le_bytes()); // compressed
        out.extend_from_slice(&(data.len() as u32).to_le_bytes()); // uncompressed
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra field length
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        out
    }

    fn fmt(bytes: &[u8], ext: Option<&str>) -> Format {
        detect(bytes, ext).format
    }

    #[test]
    fn pdf_magic() {
        assert_eq!(fmt(b"%PDF-1.7 rest", None), Format::Pdf);
        assert_eq!(detect(b"%PDF-1.7", None).confidence, Confidence::Certain);
    }

    #[test]
    fn epub_stored_mimetype_entry() {
        let mut zip = Vec::new();
        zip.extend(stored_zip_entry("mimetype", b"application/epub+zip"));
        zip.extend(stored_zip_entry("META-INF/container.xml", b"<container/>"));
        assert_eq!(fmt(&zip, None), Format::Epub);
    }

    #[test]
    fn docx_marker() {
        let zip = stored_zip_entry("word/document.xml", b"<document/>");
        assert_eq!(fmt(&zip, None), Format::Docx);
    }

    #[test]
    fn pptx_marker() {
        let zip = stored_zip_entry("ppt/presentation.xml", b"<p/>");
        assert_eq!(fmt(&zip, None), Format::Pptx);
    }

    #[test]
    fn xlsx_marker() {
        let zip = stored_zip_entry("xl/workbook.xml", b"<w/>");
        assert_eq!(fmt(&zip, None), Format::Xlsx);
    }

    #[test]
    fn odt_marker() {
        let mut zip = stored_zip_entry("mimetype", b"application/vnd.oasis.opendocument.text");
        zip.extend(stored_zip_entry("content.xml", b"<office/>"));
        assert_eq!(fmt(&zip, None), Format::Odt);
    }

    #[test]
    fn unknown_zip_layout() {
        let zip = stored_zip_entry("random.bin", b"\x00\x01");
        assert_eq!(fmt(&zip, None), Format::Unknown);
        assert_eq!(
            detect(&zip, None).evidence,
            "zip archive (unrecognized layout)"
        );
    }

    #[test]
    fn rtf_magic_and_leading_noise() {
        assert_eq!(fmt(b"{\\rtf1\\ansi Hello}", None), Format::Rtf);
        assert_eq!(fmt(b"\xEF\xBB\xBF{\\rtf1}", None), Format::Rtf); // BOM prefix
        assert_eq!(fmt(b"   \n{\\rtf1}", None), Format::Rtf);
        // Too much leading noise (> 32 bytes) -> not RTF.
        let mut noisy = vec![b' '; 40];
        noisy.extend_from_slice(b"{\\rtf1}");
        assert_ne!(fmt(&noisy, None), Format::Rtf);
    }

    #[test]
    fn html_doctype_is_certain() {
        let d = detect(b"<!DOCTYPE html><html><body>x</body></html>", None);
        assert_eq!(d.format, Format::Html);
        assert_eq!(d.confidence, Confidence::Certain);
    }

    #[test]
    fn html_bare_tags_are_guessed() {
        let d = detect(b"<html><body>x</body></html>", None);
        assert_eq!(d.format, Format::Html);
        assert_eq!(d.confidence, Confidence::Guessed);
    }

    #[test]
    fn html_case_insensitive_and_head_limited() {
        assert_eq!(fmt(b"<!DoCtYpE HtMl><p>x", None), Format::Html);
        // Markers beyond the first 1 KiB do not count (padding must be
        // non-whitespace; leading whitespace is skipped before the scan).
        let mut padded = vec![b'x'; 2048];
        padded.extend_from_slice(b"<html>");
        assert_ne!(fmt(&padded, None), Format::Html);
    }

    #[test]
    fn html_xml_declaration_then_doctype() {
        let d = detect(
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html><html/>",
            None,
        );
        assert_eq!(d.format, Format::Html);
        assert_eq!(d.confidence, Confidence::Certain);
    }

    #[test]
    fn utf8_fallback_by_extension() {
        assert_eq!(fmt(b"# hi\n", Some("md")), Format::Markdown);
        assert_eq!(fmt(b"# hi\n", Some("MD")), Format::Markdown);
        assert_eq!(fmt(b"# hi\n", Some("markdown")), Format::Markdown);
        assert_eq!(fmt(b"plain words\n", Some("txt")), Format::Text);
        assert_eq!(fmt(b"plain words\n", None), Format::Text);
        assert_eq!(fmt(b"a,b,c\n", Some("csv")), Format::Csv);
        assert_eq!(fmt(b"a\tb\tc\n", Some("tsv")), Format::Tsv);
        // Unknown extension over valid UTF-8 is still Text, never Unknown.
        assert_eq!(fmt(b"plain words\n", Some("xyz")), Format::Text);
    }

    #[test]
    fn utf8_bom_and_truncated_multibyte_at_prefix_end() {
        assert_eq!(fmt("\u{feff}hello".as_bytes(), Some("txt")), Format::Text);
        // A multi-byte char cut by the prefix boundary must not flip to Unknown.
        let mut bytes = b"abc".to_vec();
        bytes.extend_from_slice("\u{0627}".as_bytes()); // Arabic alef, 2 bytes
        bytes.pop(); // chop the second byte: unexpected end of input
        assert!(valid_utf8(&bytes));
        assert_eq!(fmt(&bytes, None), Format::Text);
    }

    #[test]
    fn invalid_utf8_is_unknown() {
        let mut bytes = b"plain".to_vec();
        bytes.extend_from_slice(&[0xff, 0xfe, 0xfd]);
        assert_eq!(fmt(&bytes, None), Format::Unknown);
        // Note: NUL-containing ASCII control bytes are still *valid* UTF-8,
        // so they detect as Text (documented heuristic; never Unknown).
        assert_eq!(fmt(b"\x00\x01\x02binary", None), Format::Text);
    }

    #[test]
    fn from_name_and_ext_roundtrip() {
        assert_eq!(Format::from_name("md"), Some(Format::Markdown));
        assert_eq!(Format::from_name(".HTML"), Some(Format::Html));
        assert_eq!(Format::from_name("markdown"), Some(Format::Markdown));
        assert_eq!(Format::from_name("pdf"), Some(Format::Pdf));
        assert_eq!(Format::from_name("docx"), Some(Format::Docx));
        assert_eq!(Format::from_name("nope"), None);
        for name in ["md", "html", "txt", "pdf", "docx", "epub", "csv"] {
            assert_eq!(
                Format::from_name(name).map(|f| f.ext()),
                Some(name),
                "ext roundtrip for {name}"
            );
        }
    }

    #[test]
    fn native_support_sets() {
        assert!(Format::Markdown.has_native_reader());
        assert!(Format::Html.has_native_writer());
        assert!(Format::Text.has_native_reader());
        assert!(!Format::Docx.has_native_reader());
        assert!(!Format::Pdf.has_native_writer());
        assert!(!Format::Unknown.has_native_reader());
    }
}
