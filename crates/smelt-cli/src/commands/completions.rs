//! `smelt completions <shell>`: print a completion script to stdout.

use clap::CommandFactory;
use clap_complete::Shell;

use crate::SmeltError;
use crate::cli::Cli;

pub fn run(shell: Shell) -> Result<(), SmeltError> {
    let mut command = Cli::command();
    clap_complete::generate(shell, &mut command, "smelt", &mut std::io::stdout());
    Ok(())
}
