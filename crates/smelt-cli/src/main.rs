//! smelt CLI entry point: parse -> dispatch -> documented exit code
//! (docs/cli.md). Per AGENTS.md this crate (and only this crate) touches
//! stdout/stderr, environment variables, and process exit.

mod cli;
mod commands;
mod engine;
mod ui;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Command};
use crate::ui::Theme;

/// A failure carrying a documented exit code (docs/cli.md). One line,
/// actionable; `main` prints it -- never a Debug dump, never a panic.
#[derive(Debug)]
pub struct SmeltError {
    pub code: u8,
    pub message: String,
}

impl SmeltError {
    pub fn new(code: u8, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Usage error: bad flags, missing `-t` where required (exit 2).
    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(2, message)
    }

    /// No reader or writer for a format (exit 3).
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(3, message)
    }

    /// Input path does not exist (exit 4).
    pub fn missing(message: impl Into<String>) -> Self {
        Self::new(4, message)
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let theme = Theme::new(cli.color);
    match run(&cli, &theme) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{}", theme.fail(&err.message));
            ExitCode::from(err.code)
        }
    }
}

/// Dispatch one parsed command. Owned here (not in `main`) so exit-code
/// handling stays in one place and tests can reason about it.
fn run(cli: &Cli, theme: &Theme) -> Result<(), SmeltError> {
    match &cli.command {
        Command::Convert(args) => commands::convert::run(args, theme, cli.color),
        Command::Info(args) => commands::info::run(args, theme),
        Command::Doctor => commands::doctor::run(theme),
        Command::Completions { shell } => commands::completions::run(*shell),
    }
}
