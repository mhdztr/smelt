//! OCR command (M3).

use std::fs;
use std::process::Command;

use crate::SmeltError;
use crate::cli::OcrArgs;

pub fn run(args: &OcrArgs) -> Result<(), SmeltError> {
    // Check for tesseract
    let which = Command::new("which").arg("tesseract").output();
    if which.is_err() || !which.unwrap().status.success() {
        return Err(SmeltError::unsupported("ocr requires tesseract on PATH"));
    }

    let _data = fs::read(&args.file)
        .map_err(|e| SmeltError::new(1, format!("cannot read {}: {}", args.file.display(), e)))?;

    // Basic OCR invocation (placeholder)
    let out = Command::new("tesseract")
        .arg(&args.file)
        .arg("stdout")
        .output()
        .map_err(|e| SmeltError::new(1, format!("tesseract failed: {}", e)))?;

    if !out.status.success() {
        return Err(SmeltError::new(1, "tesseract failed".to_string()));
    }

    let text = String::from_utf8_lossy(&out.stdout);
    print!("{}", text);
    Ok(())
}
