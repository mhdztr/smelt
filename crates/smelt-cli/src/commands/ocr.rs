//! OCR command (M3).

use std::fs;
use std::io::{self, Write};
use std::process::Command;

use smelt_core::{read_document, writer_for};
use smelt_detect::Format;

use crate::SmeltError;
use crate::cli::OcrArgs;
use crate::engine;

pub fn run(args: &OcrArgs) -> Result<(), SmeltError> {
    // Check for tesseract using pure-Rust PATH scan (zero subprocess overhead)
    if engine::which("tesseract").is_none() {
        return Err(SmeltError::unsupported("ocr requires tesseract on PATH"));
    }

    if !args.file.exists() {
        return Err(SmeltError::missing(format!(
            "cannot read {}: No such file or directory",
            args.file.display()
        )));
    }

    // Basic OCR invocation
    let out = Command::new("tesseract")
        .arg(&args.file)
        .arg("stdout")
        .output()
        .map_err(|e| SmeltError::new(1, format!("tesseract failed: {}", e)))?;

    if !out.status.success() {
        let err_msg = String::from_utf8_lossy(&out.stderr);
        return Err(SmeltError::new(
            1,
            format!("tesseract failed: {}", err_msg.trim()),
        ));
    }

    let text = String::from_utf8_lossy(&out.stdout);

    if let Some(ref target_str) = args.to {
        let target_fmt = Format::from_name(target_str)
            .ok_or_else(|| SmeltError::usage(format!("unknown target format '{target_str}'")))?;

        let doc = read_document(Format::Markdown, text.as_bytes())
            .map_err(|e| SmeltError::new(1, format!("ocr parse error: {e}")))?;

        let writer = writer_for(target_fmt).ok_or_else(|| {
            SmeltError::unsupported(format!("no writer available for {target_fmt:?}"))
        })?;

        let bytes = writer
            .write(&doc, target_fmt)
            .map_err(|e| SmeltError::new(1, format!("ocr write error: {e}")))?;

        if let Some(ref out_path) = args.output {
            fs::write(out_path, &bytes).map_err(|e| {
                SmeltError::new(1, format!("cannot write {}: {}", out_path.display(), e))
            })?;
        } else {
            let mut stdout = io::stdout().lock();
            stdout
                .write_all(&bytes)
                .map_err(|e| SmeltError::new(1, format!("stdout write error: {e}")))?;
        }
    } else if let Some(ref out_path) = args.output {
        fs::write(out_path, text.as_bytes()).map_err(|e| {
            SmeltError::new(1, format!("cannot write {}: {}", out_path.display(), e))
        })?;
    } else {
        print!("{}", text);
    }

    Ok(())
}
