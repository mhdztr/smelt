//! clap derive definitions: the CLI surface exactly as specified in
//! `docs/cli.md`. Parsing and help text only -- behavior lives in
//! `commands/`.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

use crate::ui::ColorChoice;

const CONVERT_AFTER_HELP: &str = r#"Examples:
  smelt convert notes.md -o notes.html
  smelt convert docs/ -t md -o out/
  printf '# Hi\n\n**bold**\n' | smelt convert - -t html"#;

const INFO_AFTER_HELP: &str = r#"Examples:
  smelt info README.md
  smelt info README.md --json
  printf '# Hi\n' | smelt info -"#;

const DOCTOR_AFTER_HELP: &str = "\
Examples:
  smelt doctor";

const COMPLETIONS_AFTER_HELP: &str = "\
Examples:
  smelt completions bash > ~/.local/share/bash-completion/completions/smelt";

/// Melt any document into any other.
#[derive(Debug, Parser)]
#[command(
    name = "smelt",
    version,
    about = "Melt any document into any other",
    after_help = "Run 'smelt <COMMAND> --help' for details on any command."
)]
pub struct Cli {
    /// When to colorize output (auto, always, never)
    #[arg(long, global = true, value_name = "WHEN", default_value = "auto")]
    pub color: ColorChoice,

    #[command(subcommand)]
    pub command: Command,
}

/// M0 subcommands (`watch`, `diff`, `ocr`, `man` are planned, docs/cli.md).
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Convert files, directories, or stdin to a target format
    #[command(visible_alias = "c", after_help = CONVERT_AFTER_HELP)]
    Convert(ConvertArgs),

    /// Detect the format of a file and print stats
    #[command(after_help = INFO_AFTER_HELP)]
    Info(InfoArgs),

    /// Report which optional external engines are on PATH
    #[command(after_help = DOCTOR_AFTER_HELP)]
    Doctor,

    /// Print a shell completion script to stdout
    #[command(after_help = COMPLETIONS_AFTER_HELP)]
    Completions {
        /// Shell to generate the script for
        shell: Shell,
    },
}

#[derive(Debug, Args)]
pub struct ConvertArgs {
    /// Input file, directory, or - for stdin (default: - when piped)
    #[arg(value_name = "INPUT")]
    pub input: Option<PathBuf>,

    /// Target format name (md, html, txt, ...)
    #[arg(short = 't', long = "to", value_name = "FMT")]
    pub to: Option<String>,

    /// Output file, directory, or - for stdout
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Suppress progress, per-file lines, and the summary box
    #[arg(short, long)]
    pub quiet: bool,

    /// Extra detail (detection evidence per file in batch mode)
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Debug, Args)]
pub struct InfoArgs {
    /// File path or - for stdin (default: - when piped)
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Machine-readable JSON output
    #[arg(long)]
    pub json: bool,
}
