//! Pandoc engine pool (M3).

use std::io::Write;
use std::process::{Command, Stdio};

use crate::error::{PoolError, PoolResult};
use crate::pool::{EnginePool, PoolHealth};

pub struct PandocPool;

impl Default for PandocPool {
    fn default() -> Self {
        Self::new()
    }
}

impl PandocPool {
    pub fn new() -> Self {
        Self
    }

    pub fn convert(&self, input: &[u8], from_ext: &str, to_ext: &str) -> PoolResult<Vec<u8>> {
        if !check_pandoc_preconditions() {
            return Err(PoolError::Message("pandoc not found on PATH".to_string()));
        }

        let from_format = match from_ext {
            "tex" | "latex" => "latex",
            "md" | "markdown" => "markdown",
            other => other,
        };

        let to_format = match to_ext {
            "tex" | "latex" => "latex",
            "md" | "markdown" => "markdown",
            other => other,
        };

        let mut child = Command::new("pandoc")
            .arg("-f")
            .arg(from_format)
            .arg("-t")
            .arg(to_format)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(PoolError::Io)?;

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input);
        }

        let output = child.wait_with_output().map_err(PoolError::Io)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PoolError::Message(format!(
                "pandoc conversion failed: {}",
                stderr.trim()
            )));
        }

        Ok(output.stdout)
    }
}

impl EnginePool for PandocPool {
    fn name(&self) -> &'static str {
        "pandoc"
    }

    fn health(&self) -> PoolResult<PoolHealth> {
        let which = std::process::Command::new("which").arg("pandoc").output();
        match which {
            Ok(out) if out.status.success() => Ok(PoolHealth {
                ok: true,
                workers: 1,
                message: Some("ready".to_string()),
            }),
            _ => Ok(PoolHealth {
                ok: false,
                workers: 0,
                message: Some("binary not found on PATH".to_string()),
            }),
        }
    }

    fn shutdown(&self) {}
}

pub fn check_pandoc_preconditions() -> bool {
    let out = std::process::Command::new("which").arg("pandoc").output();
    matches!(out, Ok(o) if o.status.success())
}
