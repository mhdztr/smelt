//! `smelt info`: detect a file's (or stdin's) format and print stats.
//! Behavior of record: docs/cli.md (`--json` shape, null rules, exit codes).

use std::io::IsTerminal;
use std::path::Path;

use smelt_core::Format;
use smelt_detect::{Confidence, detect, detect_path};

use crate::SmeltError;
use crate::cli::InfoArgs;
use crate::ui::{Theme, human_bytes};

/// Label column width from docs/cli.md's human-output sample.
const LABEL_WIDTH: usize = 10;

pub fn run(args: &InfoArgs, theme: &Theme) -> Result<(), SmeltError> {
    let from_stdin = match &args.file {
        Some(path) => path == Path::new("-"),
        // Default to stdin when piped; a terminal without an argument is
        // a usage error (there is nothing to inspect).
        None => !std::io::stdin().is_terminal(),
    };
    if from_stdin {
        run_stdin(args, theme)
    } else {
        run_file(
            args,
            theme,
            args.file.as_deref().expect("file set when not stdin"),
        )
    }
}

fn run_file(args: &InfoArgs, theme: &Theme, path: &Path) -> Result<(), SmeltError> {
    let detection = detect_path(path)
        .map_err(|_| SmeltError::missing(format!("no such file: {}", path.display())))?;

    let size_bytes = std::fs::metadata(path).map(|m| m.len()).ok();
    let stats = text_stats(path, detection.format);

    print_report(
        args,
        theme,
        &path.display().to_string(),
        detection.format,
        detection.confidence,
        detection.evidence,
        size_bytes,
        stats,
    )
}

fn run_stdin(args: &InfoArgs, theme: &Theme) -> Result<(), SmeltError> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::io::stdin()
        .read_to_end(&mut bytes)
        .map_err(|e| SmeltError::new(1, format!("failed: read stdin ({e})")))?;

    let detection = detect(&bytes, None);
    let stats = stats_from(&bytes, detection.format);

    // size_bytes is null for stdin (docs/cli.md): unknown until read, and
    // the value would disagree with the file-size semantics used elsewhere.
    print_report(
        args,
        theme,
        "-",
        detection.format,
        detection.confidence,
        detection.evidence,
        None,
        stats,
    )
}

/// lines/words/bytes for text formats only; `None` (JSON null) otherwise.
fn text_stats(path: &Path, fmt: Format) -> Option<Stats> {
    if !fmt.has_native_reader() {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    stats_from(&bytes, fmt)
}

fn stats_from(bytes: &[u8], fmt: Format) -> Option<Stats> {
    if !fmt.has_native_reader() {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    Some(Stats {
        lines: text.lines().count(),
        words: text.split_whitespace().count(),
        bytes: bytes.len(),
    })
}

struct Stats {
    lines: usize,
    words: usize,
    bytes: usize,
}

#[allow(clippy::too_many_arguments)]
fn print_report(
    args: &InfoArgs,
    theme: &Theme,
    path: &str,
    fmt: Format,
    confidence: Confidence,
    evidence: &str,
    size_bytes: Option<u64>,
    stats: Option<Stats>,
) -> Result<(), SmeltError> {
    let confidence = match confidence {
        Confidence::Certain => "certain",
        Confidence::Guessed => "guessed",
    };

    if args.json {
        let value = serde_json::json!({
            "path": path,
            "format": fmt.name(),
            "confidence": confidence,
            "evidence": evidence,
            "size_bytes": size_bytes,
            "lines": stats.as_ref().map(|s| s.lines),
            "words": stats.as_ref().map(|s| s.words),
            "bytes": stats.as_ref().map(|s| s.bytes),
        });
        let rendered = serde_json::to_string_pretty(&value)
            .map_err(|e| SmeltError::new(1, format!("failed: render json ({e})")))?;
        println!("{rendered}");
        return Ok(());
    }

    let label = |name: &str| format!("{name:LABEL_WIDTH$}");
    println!("{}{}", label("path"), theme.bold(path));
    println!(
        "{}{} ({confidence}: {evidence})",
        label("format"),
        fmt.name()
    );
    if let Some(size) = size_bytes {
        println!("{}{}", label("size"), human_bytes(size));
    }
    if let Some(stats) = stats {
        println!("{}{}", label("lines"), stats.lines);
        println!("{}{}", label("words"), stats.words);
        println!("{}{}", label("bytes"), stats.bytes);
    }
    Ok(())
}
