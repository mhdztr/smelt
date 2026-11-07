//! WASM bindings for smelt document conversion.
//!
//! Browser-safe bindings over `smelt-core`. No filesystem, no external
//! engines, no threads, no panics on user input. Supported formats depend on
//! which `smelt-core` features are compiled into this build.

use wasm_bindgen::prelude::*;

use smelt_core::{Format, read_document, reader_for, writer_for};
use smelt_detect::detect;

fn core_error(msg: impl std::fmt::Display) -> String {
    msg.to_string()
}

fn supported_format_exts() -> Vec<&'static str> {
    let mut names = Vec::new();
    for fmt in [
        Format::Markdown,
        Format::Html,
        Format::Text,
        Format::Csv,
        Format::Tsv,
        Format::Docx,
        Format::Odt,
        Format::Epub,
        Format::Pdf,
        Format::Rtf,
    ] {
        if reader_for(fmt).is_some() && writer_for(fmt).is_some() {
            names.push(fmt.ext());
        }
    }
    names
}

fn detect_format_impl(input: &[u8], filename: Option<&str>) -> String {
    let ext = filename
        .and_then(|name| std::path::Path::new(name).extension())
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    let detection = detect(input, ext.as_deref());
    if detection.format == Format::Unknown {
        "unknown".to_string()
    } else {
        detection.format.ext().to_string()
    }
}

fn resolve_source(
    from: Option<&str>,
    filename: Option<&str>,
    input: &[u8],
) -> Result<Format, String> {
    if let Some(name) = from.filter(|s| !s.trim().is_empty() && *s != "auto") {
        return Format::from_name(name)
            .ok_or_else(|| core_error(format!("unknown source format: {name}")));
    }
    let ext = filename
        .and_then(|name| std::path::Path::new(name).extension())
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    let detection = detect(input, ext.as_deref());
    Ok(detection.format)
}

/// Match CLI parity: plain text detection is read through the Markdown reader.
fn source_reader_format(detected: Format) -> Format {
    if detected == Format::Text {
        Format::Markdown
    } else {
        detected
    }
}

fn resolve_target(to: &str) -> Result<Format, String> {
    Format::from_name(to).ok_or_else(|| core_error(format!("unknown target format: {to}")))
}

fn convert_impl(
    input: &[u8],
    from: Option<&str>,
    to: &str,
    filename: Option<&str>,
) -> Result<Vec<u8>, String> {
    let detected = resolve_source(from, filename, input)?;
    let source = source_reader_format(detected);
    let target = resolve_target(to)?;

    let reader_name = || source.name().to_lowercase();
    if reader_for(source).is_none() {
        return Err(core_error(format!(
            "no native reader for {} in this build",
            reader_name()
        )));
    }
    if writer_for(target).is_none() {
        return Err(core_error(format!(
            "no native writer for {} in this build",
            target.name().to_lowercase()
        )));
    }

    let doc = read_document(source, input)
        .map_err(|e| core_error(format!("read {}: {e}", reader_name())))?;
    let writer = writer_for(target).ok_or_else(|| {
        core_error(format!(
            "no native writer for {} in this build",
            target.name().to_lowercase()
        ))
    })?;
    writer
        .write(&doc, target)
        .map_err(|e| core_error(format!("write {}: {e}", target.name().to_lowercase())))
}

fn js_err(msg: String) -> JsValue {
    JsValue::from_str(&msg)
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// JSON array of canonical format names supported for both reading and writing
/// in this WASM build.
#[wasm_bindgen]
pub fn supported_formats() -> String {
    let names: Vec<String> = supported_format_exts()
        .into_iter()
        .map(|n| format!("\"{n}\""))
        .collect();
    format!("[{}]", names.join(","))
}

/// Detect a document format from bytes and an optional filename hint.
/// Returns the canonical format extension (e.g. `md`, `html`) or `unknown`.
#[wasm_bindgen]
pub fn detect_format(input: &[u8], filename: Option<String>) -> String {
    detect_format_impl(input, filename.as_deref())
}

/// Convert document bytes from one format to another.
/// `from`: source format name (e.g. `md`, `html`) or `None`/`"auto"` to detect.
/// `to`: target format name.
/// `filename`: optional filename hint for detection/disambiguation.
/// Returns converted bytes. Use `convert_text` for UTF-8 text convenience.
#[wasm_bindgen]
pub fn convert(
    input: &[u8],
    from: Option<String>,
    to: String,
    filename: Option<String>,
) -> Result<Vec<u8>, JsValue> {
    convert_impl(input, from.as_deref(), &to, filename.as_deref()).map_err(js_err)
}

/// Convenience wrapper for UTF-8 text input/output.
#[wasm_bindgen]
pub fn convert_text(
    input: &str,
    from: Option<String>,
    to: String,
    filename: Option<String>,
) -> Result<String, JsValue> {
    let out = convert(input.as_bytes(), from, to, filename)?;
    String::from_utf8(out).map_err(|_| {
        js_err("output is not valid UTF-8; use convert() for binary targets".to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_semver() {
        let v = version();
        assert!(v.contains('.'), "version should look like semver: {v}");
    }

    #[test]
    fn supported_formats_json_array() {
        let s = supported_formats();
        assert!(s.starts_with('[') && s.ends_with(']'));
        assert!(s.contains("\"md\""));
        assert!(s.contains("\"html\""));
        assert!(s.contains("\"txt\""));
    }

    #[test]
    fn detect_md() {
        assert_eq!(detect_format_impl(b"# Hi\n", Some("a.md")), "md");
    }

    #[test]
    fn convert_md_to_html() {
        let out = convert_impl(b"# Hi\n\n**bold**\n", Some("md"), "html", None).expect("convert");
        let html = String::from_utf8(out).unwrap();
        assert!(html.contains("<h1>Hi</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
    }

    #[test]
    fn convert_html_to_md() {
        let out =
            convert_impl(b"<h1>Hi</h1><p>text</p>", Some("html"), "md", None).expect("convert");
        let md = String::from_utf8(out).unwrap();
        assert!(md.contains("# Hi"));
    }

    #[test]
    fn convert_auto_detect() {
        let out = convert_impl(b"# Hi\n\ntext\n", None, "html", Some("note.md")).expect("convert");
        let html = String::from_utf8(out).unwrap();
        assert!(html.contains("<h1>Hi</h1>"));
    }

    #[test]
    fn unknown_target_errors() {
        let err = convert_impl(b"hi", Some("txt"), "nope", None).expect_err("should fail");
        assert!(err.contains("unknown target format"));
    }

    #[test]
    fn unsupported_in_build_errors_actionably() {
        // office formats like ods have no native reader in any build
        let err = convert_impl(b"x", Some("ods"), "html", None).expect_err("should fail");
        assert!(err.contains("no native reader") || err.contains("unknown source"));
    }

    #[test]
    fn csv_roundtrip() {
        let out = convert_impl(b"a,b\n1,2\n", Some("csv"), "tsv", None).expect("convert");
        let tsv = String::from_utf8(out).unwrap();
        assert!(tsv.contains("a\tb"));
    }
}
