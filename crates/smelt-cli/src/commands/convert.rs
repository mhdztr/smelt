//! `smelt convert`: files, directories (batch), and stdin -> target format.
//! Behavior of record: docs/cli.md (output-format resolution table, batch
//! rules, exit codes).

use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use rayon::prelude::*;

use smelt_core::{Format, ReadError, WriteError, read_document, reader_for, writer_for};
use smelt_detect::{detect, detect_path};

use crate::SmeltError;
use crate::cli::ConvertArgs;
use crate::commands::{io_error, io_reason};
use crate::ui::{ColorChoice, Theme, human_bytes, human_duration};

pub fn run(args: &ConvertArgs, theme: &Theme, color: ColorChoice) -> Result<(), SmeltError> {
    let input = match &args.input {
        Some(path) => path.clone(),
        // Default `-` when stdin is piped; a terminal needs an argument.
        None if !std::io::stdin().is_terminal() => PathBuf::from("-"),
        None => {
            return Err(SmeltError::usage(
                "missing input: pass a file, directory, or - for stdin",
            ));
        }
    };

    if input == Path::new("-") {
        return run_stdin(args, theme);
    }

    match std::fs::metadata(&input) {
        Err(e) => Err(io_error(&e, &input)),
        Ok(meta) if meta.is_dir() => run_batch(args, theme, color, &input),
        Ok(_) => run_file(args, theme, &input),
    }
}

/// Output-format resolution for a single file (docs/cli.md): `-t` wins;
/// otherwise a known extension on `-o` infers the format; otherwise usage
/// error. `out_path` is the resolved output path (or `None` when the caller
/// should derive it from the input path).
fn file_target(args: &ConvertArgs, input: &Path) -> Result<(Format, PathBuf), SmeltError> {
    if let Some(name) = &args.to {
        let target = Format::from_name(name)
            .ok_or_else(|| SmeltError::usage(format!("unknown format: {name}")))?;
        let out_path = match &args.output {
            Some(p) if p != Path::new("-") => p.clone(),
            _ => input.with_extension(target.ext()),
        };
        return Ok((target, out_path));
    }

    match &args.output {
        Some(p) if p != Path::new("-") => {
            let inferred = p
                .extension()
                .and_then(|e| e.to_str())
                .and_then(Format::from_name);
            match inferred {
                Some(target) => Ok((target, p.clone())),
                None => Err(SmeltError::usage(format!(
                    "pass -t/--to or -o/--output to choose the output format: \
                     cannot infer a format from {}",
                    p.display()
                ))),
            }
        }
        _ => Err(SmeltError::usage(
            "pass -t/--to or -o/--output to choose the output format",
        )),
    }
}

fn target_writer(fmt: Format) -> Result<&'static dyn smelt_core::Writer, SmeltError> {
    writer_for(fmt).ok_or_else(|| {
        SmeltError::unsupported(WriteError::Unsupported(fmt.name().to_lowercase()).to_string())
    })
}

/// Stdin: `-t` is required (no extension to infer from). Source detection
/// runs on the bytes; when it lands on plain `Text`, the Markdown reader is
/// used -- markdown is a superset of plain text, so `# Hi` and `**bold**`
/// from a pipe convert properly instead of coming out verbatim.
fn run_stdin(args: &ConvertArgs, theme: &Theme) -> Result<(), SmeltError> {
    let Some(name) = &args.to else {
        return Err(SmeltError::usage("stdin requires -t"));
    };
    let target = Format::from_name(name)
        .ok_or_else(|| SmeltError::usage(format!("unknown format: {name}")))?;

    let mut bytes = Vec::new();
    std::io::stdin()
        .read_to_end(&mut bytes)
        .map_err(|e| SmeltError::new(1, format!("failed: read stdin ({e})")))?;

    let detection = detect(&bytes, None);
    let source = source_format(detection.format);

    let started = Instant::now();
    let doc = read_document(source, &bytes)
        .map_err(|e| SmeltError::new(exit_code_for(&e), format!("failed: stdin ({e})")))?;
    let out = target_writer(target)?
        .write(&doc, target)
        .map_err(|e| SmeltError::new(exit_code_for_write(&e), format!("failed: stdin ({e})")))?;
    let ms = started.elapsed().as_millis();

    let out_name = match &args.output {
        Some(p) if p != Path::new("-") => {
            write_output_file(p, &out)?;
            p.display().to_string()
        }
        _ => {
            let mut stdout = std::io::stdout();
            stdout
                .write_all(&out)
                .map_err(|e| SmeltError::new(1, format!("failed: write stdout ({e})")))?;
            let _ = stdout.flush();
            "-".to_string()
        }
    };

    if !args.quiet {
        print_success(theme, "-", &out_name, out.len(), ms);
    }
    Ok(())
}

/// The reader used for a source format: detected plain `Text` is read with
/// the **Markdown** reader. Markdown is a superset of plain text, and the
/// plain-text reader collapses whitespace and keeps every marker literal
/// (`**bold**` stays `**bold**`, then `TextWriter` drops the emphasis
/// markers), so reading plain text as text destroys content as soon as the
/// target is not plain text. Shared by the stdin and file paths so both
/// channels interpret identical bytes identically (docs/cli.md).
fn source_format(detected: Format) -> Format {
    if detected == Format::Text {
        Format::Markdown
    } else {
        detected
    }
}

/// One file: detect -> read -> write. Output path is `-o`, else the input
/// path with the target extension.
fn run_file(args: &ConvertArgs, theme: &Theme, input: &Path) -> Result<(), SmeltError> {
    let (target, out_path) = file_target(args, input)?;
    let to_stdout = args.output.as_deref() == Some(Path::new("-"));
    if !to_stdout {
        refuse_in_place(input, &out_path)?;
    }

    let detection = detect_path(input).map_err(|e| io_error(&e, input))?;
    // Same source-format resolution as stdin (docs/cli.md stdin rule): plain
    // text is read through the Markdown reader on both channels, so the same
    // bytes convert to the same result whichever way they arrive.
    let source = source_format(detection.format);

    let bytes = std::fs::read(input)
        .map_err(|e| SmeltError::new(1, format!("failed: read {} ({e})", input.display())))?;

    let started = Instant::now();
    let doc = read_document(source, &bytes).map_err(|e| {
        SmeltError::new(
            exit_code_for(&e),
            format!(
                "failed: {} -> {} ({e})",
                input.display(),
                out_path.display()
            ),
        )
    })?;
    let out = target_writer(target)?.write(&doc, target).map_err(|e| {
        SmeltError::new(
            exit_code_for_write(&e),
            format!(
                "failed: {} -> {} ({e})",
                input.display(),
                out_path.display()
            ),
        )
    })?;
    let ms = started.elapsed().as_millis();

    if to_stdout {
        let mut stdout = std::io::stdout();
        stdout
            .write_all(&out)
            .map_err(|e| SmeltError::new(1, format!("failed: write stdout ({e})")))?;
        let _ = stdout.flush();
    } else {
        write_output_file(&out_path, &out)?;
    }

    if !args.quiet {
        let in_name = file_name(input);
        let out_name = if to_stdout {
            "-".to_string()
        } else {
            file_name(&out_path)
        };
        print_success(theme, &in_name, &out_name, out.len(), ms);
    }
    Ok(())
}

/// Batch: every regular file directly inside `input`, parallel (rayon),
/// output into `-o` directory. Non-recursive in M0 (docs/cli.md).
fn run_batch(
    args: &ConvertArgs,
    theme: &Theme,
    color: ColorChoice,
    input: &Path,
) -> Result<(), SmeltError> {
    let Some(target_name) = &args.to else {
        return Err(SmeltError::usage("directory input requires -t"));
    };
    let target = Format::from_name(target_name)
        .ok_or_else(|| SmeltError::usage(format!("unknown format: {target_name}")))?;

    let out_dir = match &args.output {
        Some(p) if p != Path::new("-") => p.clone(),
        _ => {
            return Err(SmeltError::usage(
                "stdout forbids directory input: pass -o with a directory",
            ));
        }
    };
    if out_dir.exists() && !out_dir.is_dir() {
        return Err(SmeltError::usage(format!(
            "output must be a directory for batch conversion: {}",
            out_dir.display()
        )));
    }
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| SmeltError::new(1, format!("failed: create {} ({e})", out_dir.display())))?;

    let mut files = Vec::new();
    let entries = std::fs::read_dir(input)
        .map_err(|e| SmeltError::new(1, format!("failed: read {} ({e})", input.display())))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let hidden = name.to_string_lossy().starts_with('.');
        if hidden || !entry.path().is_file() {
            continue;
        }
        files.push(entry.path());
    }
    files.sort();

    // Collision check before any write: two inputs with the same stem and a
    // different extension (`notes.md` + `notes.html`) map to the same output
    // name. The writes run in parallel under rayon, so silently continuing
    // would leave a nondeterministic winner and exit 0 (docs/cli.md: exit 2).
    check_batch_collisions(&files, &out_dir, target)?;

    let use_bars = !args.quiet && color != ColorChoice::Never && std::io::stderr().is_terminal();
    let multi = indicatif::MultiProgress::new();
    let bars: Vec<indicatif::ProgressBar> = if use_bars {
        files
            .iter()
            .map(|path| {
                let bar = multi.add(indicatif::ProgressBar::new(1));
                bar.set_style(
                    indicatif::ProgressStyle::with_template("{msg}")
                        .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar()),
                );
                bar.set_message(format!("converting {}", file_name(path)));
                bar
            })
            .collect()
    } else {
        Vec::new()
    };

    let started = Instant::now();
    let outcomes: Vec<FileOutcome> = files
        .par_iter()
        .enumerate()
        .map(|(index, path)| {
            let outcome = convert_batch_file(path, &out_dir, target);
            let name = file_name(path);
            if use_bars {
                bars[index].finish_with_message(outcome.line(&name, args.verbose));
            } else if !args.quiet {
                eprintln!("{}", outcome.line(&name, args.verbose));
            }
            outcome
        })
        .collect();
    let ms = started.elapsed().as_millis();

    let total = outcomes.len();
    let failed = outcomes.iter().filter(|o| o.is_failure()).count();
    let unsupported_only = outcomes.iter().all(|o| {
        matches!(
            o,
            FileOutcome::Failed {
                unsupported: true,
                ..
            }
        )
    });

    if !args.quiet {
        print_summary(theme, total, total - failed, failed, ms);
    }

    if failed == 0 {
        Ok(())
    } else if total - failed > 0 {
        Err(SmeltError::new(
            1,
            format!("{failed} of {total} conversions failed"),
        ))
    } else if unsupported_only {
        Err(SmeltError::unsupported(format!(
            "{failed} of {total} conversions failed (unsupported format)"
        )))
    } else {
        Err(SmeltError::new(
            1,
            format!("{failed} of {total} conversions failed"),
        ))
    }
}

enum FileOutcome {
    Ok {
        out_name: String,
        ms: u128,
        evidence: &'static str,
    },
    Failed {
        message: String,
        unsupported: bool,
    },
}

impl FileOutcome {
    fn is_failure(&self) -> bool {
        matches!(self, FileOutcome::Failed { .. })
    }

    fn line(&self, in_name: &str, verbose: bool) -> String {
        match self {
            FileOutcome::Ok {
                out_name,
                ms,
                evidence,
            } => {
                let mut line = format!("converting {in_name} -> {out_name} ... ok ({ms} ms)");
                if verbose {
                    line.push_str(&format!(" ({evidence})"));
                }
                line
            }
            FileOutcome::Failed { message, .. } => {
                format!("converting {in_name} ... failed ({message})")
            }
        }
    }
}

/// Refuse a conversion whose resolved output is the input itself. Rewriting
/// the source through the IDM is destructive (classes, styles, and structure
/// the model does not carry are lost) and used to happen silently at exit 0
/// for `convert a.html -t html` and for batch output into the input directory
/// (docs/cli.md). There is no `--force` in M0: pass `-o` with a distinct path
/// or a different `-t`.
fn refuse_in_place(input: &Path, out_path: &Path) -> Result<(), SmeltError> {
    if paths_are_same(input, out_path) {
        return Err(SmeltError::usage(format!(
            "refusing to overwrite the input file in place: {}; \
             pass -o with a distinct output path or a different -t",
            input.display()
        )));
    }
    Ok(())
}

/// Whether two paths name the same file. Exact path equality catches the
/// extension-derived case (`a.md` -> `a.md`); canonicalization catches the
/// spelling variants (`a.md` vs `./a.md`) when both already exist.
fn paths_are_same(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// The output path for one batch input: the output directory plus the input
/// stem and the target extension. Single source of truth for the mapping
/// [`check_batch_collisions`] validates.
fn batch_out_path(path: &Path, out_dir: &Path, target: Format) -> PathBuf {
    out_dir.join(format!(
        "{}.{}",
        path.file_stem().and_then(|s| s.to_str()).unwrap_or("out"),
        target.ext()
    ))
}

/// Refuse a batch whose inputs would map to the same output file (exit 2,
/// before anything is written). Without this the parallel rayon writes race
/// and one input silently overwrites the other's output. Also refuses an
/// output path that resolves to its own input (batch into the source
/// directory with the source extension), for the same reason the single-file
/// path does.
fn check_batch_collisions(
    files: &[PathBuf],
    out_dir: &Path,
    target: Format,
) -> Result<(), SmeltError> {
    let mut seen: std::collections::BTreeMap<PathBuf, &Path> = std::collections::BTreeMap::new();
    for path in files {
        let out_path = batch_out_path(path, out_dir, target);
        refuse_in_place(path, &out_path)?;
        if let Some(previous) = seen.insert(out_path.clone(), path.as_path()) {
            return Err(SmeltError::usage(format!(
                "batch output collision: {} and {} both map to {}; \
                 rename one input or convert them separately",
                previous.display(),
                path.display(),
                out_path.display()
            )));
        }
    }
    Ok(())
}

fn convert_batch_file(path: &Path, out_dir: &Path, target: Format) -> FileOutcome {
    let in_name = file_name(path);
    let out_path = batch_out_path(path, out_dir, target);
    let out_name = file_name(&out_path);

    let detection = match detect_path(path) {
        Ok(d) => d,
        Err(e) => {
            return FileOutcome::Failed {
                message: format!("read {in_name} ({})", io_reason(&e)),
                unsupported: false,
            };
        }
    };
    // Same source-format resolution as stdin and single-file conversion
    // (docs/cli.md): detection landing on plain `Text` is read through the
    // Markdown reader. Batch files are a channel too, so this must not use
    // the raw detection result (it would emit literal Markdown inside HTML).
    let source = source_format(detection.format);
    if reader_for(source).is_none() {
        return FileOutcome::Failed {
            message: ReadError::Unsupported(source.name().to_lowercase()).to_string(),
            unsupported: true,
        };
    };
    let Some(writer) = writer_for(target) else {
        return FileOutcome::Failed {
            message: WriteError::Unsupported(target.name().to_lowercase()).to_string(),
            unsupported: true,
        };
    };

    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            return FileOutcome::Failed {
                message: format!("read {in_name} ({e})"),
                unsupported: false,
            };
        }
    };

    let started = Instant::now();
    let doc = match read_document(source, &bytes) {
        Ok(d) => d,
        Err(e) => {
            return FileOutcome::Failed {
                message: format!("{in_name} -> {out_name} ({e})"),
                unsupported: e.is_unsupported(),
            };
        }
    };
    let out = match writer.write(&doc, target) {
        Ok(o) => o,
        Err(e) => {
            return FileOutcome::Failed {
                message: format!("{in_name} -> {out_name} ({e})"),
                unsupported: e.is_unsupported(),
            };
        }
    };
    let ms = started.elapsed().as_millis();

    if let Err(e) = std::fs::write(&out_path, &out) {
        return FileOutcome::Failed {
            message: format!("write {out_name} ({e})"),
            unsupported: false,
        };
    }

    FileOutcome::Ok {
        out_name,
        ms,
        evidence: detection.evidence,
    }
}

fn write_output_file(path: &Path, out: &[u8]) -> Result<(), SmeltError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent).map_err(|e| {
            SmeltError::new(1, format!("failed: create {} ({e})", parent.display()))
        })?;
    }
    std::fs::write(path, out)
        .map_err(|e| SmeltError::new(1, format!("failed: write {} ({e})", path.display())))
}

/// `a.md -> a.html (3.1 KB, 4 ms)` to stderr (DESIGN.md sample): filenames
/// bold, trailing detail dim.
fn print_success(theme: &Theme, in_name: &str, out_name: &str, bytes: usize, ms: u128) {
    let detail = format!("({}, {})", human_bytes(bytes as u64), human_duration(ms));
    eprintln!(
        "{} -> {} {}",
        theme.bold(in_name),
        theme.bold(out_name),
        theme.dim(&detail)
    );
}

/// Summary box (DESIGN.md): labels padded to the widest label, values
/// left-aligned, box padded to content width. ASCII fallback via Theme.
fn print_summary(theme: &Theme, converted: usize, ok: usize, failed: usize, ms: u128) {
    let rows: [(&str, String); 4] = [
        ("converted", format!("{converted} files")),
        ("ok", ok.to_string()),
        ("failed", failed.to_string()),
        ("elapsed", human_duration(ms)),
    ];
    let label_w = rows.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
    let value_w = rows.iter().map(|(_, v)| v.len()).max().unwrap_or(0);
    let inner_w = 1 + label_w + 3 + value_w + 1;
    let b = theme.box_chars();

    eprintln!("{}{}{}", b.tl, b.h.repeat(inner_w), b.tr);
    for (label, value) in &rows {
        eprintln!(
            "{} {:<label_w$}   {:<value_w$} {}",
            b.v,
            label,
            value,
            b.v,
            label_w = label_w,
            value_w = value_w
        );
    }
    eprintln!("{}{}{}", b.bl, b.h.repeat(inner_w), b.br);
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn exit_code_for(err: &ReadError) -> u8 {
    if err.is_unsupported() { 3 } else { 1 }
}

fn exit_code_for_write(err: &WriteError) -> u8 {
    if err.is_unsupported() { 3 } else { 1 }
}
