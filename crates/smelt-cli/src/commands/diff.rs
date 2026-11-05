//! Diff command (M3).

use std::fs;

use smelt_core::{Format, read_document};

use crate::SmeltError;
use crate::cli::DiffArgs;

pub fn run(args: &DiffArgs) -> Result<(), SmeltError> {
    let b1 = fs::read(&args.file1)
        .map_err(|e| SmeltError::new(1, format!("cannot read {}: {}", args.file1.display(), e)))?;
    let b2 = fs::read(&args.file2)
        .map_err(|e| SmeltError::new(1, format!("cannot read {}: {}", args.file2.display(), e)))?;

    // Try to detect and parse
    let fmt1 = smelt_detect::detect_path(&args.file1)
        .map(|d| d.format)
        .unwrap_or(Format::Text);
    let fmt2 = smelt_detect::detect_path(&args.file2)
        .map(|d| d.format)
        .unwrap_or(Format::Text);

    let doc1 = read_document(fmt1, &b1).map_err(|e| SmeltError::new(1, format!("diff: {}", e)))?;
    let doc2 = read_document(fmt2, &b2).map_err(|e| SmeltError::new(1, format!("diff: {}", e)))?;

    if doc1 == doc2 {
        println!("identical");
    } else {
        println!(
            "different ({} blocks vs {} blocks)",
            doc1.blocks.len(),
            doc2.blocks.len()
        );
    }
    Ok(())
}
