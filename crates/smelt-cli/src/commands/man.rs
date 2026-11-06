//! Man page command (M4).

use clap::CommandFactory;

use crate::SmeltError;
use crate::cli::Cli;

pub fn run() -> Result<(), SmeltError> {
    let cmd = Cli::command();
    let man = clap_mangen::Man::new(cmd);
    let mut buffer: Vec<u8> = Vec::new();
    man.render(&mut buffer)
        .map_err(|e| SmeltError::new(1, format!("man page generation failed: {}", e)))?;
    use std::io::Write;
    std::io::stdout()
        .write_all(&buffer)
        .map_err(|e| SmeltError::new(1, format!("write failed: {}", e)))?;
    Ok(())
}
