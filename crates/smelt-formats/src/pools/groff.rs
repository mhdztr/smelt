//! Groff engine pool (M3).

use std::io::Write;
use std::process::{Command, Stdio};

use crate::error::{PoolError, PoolResult};
use crate::pool::{EnginePool, PoolHealth};

pub struct GroffPool;

impl Default for GroffPool {
    fn default() -> Self {
        Self::new()
    }
}

impl GroffPool {
    pub fn new() -> Self {
        Self
    }

    pub fn convert(&self, input: &[u8], _from_ext: &str, to_ext: &str) -> PoolResult<Vec<u8>> {
        if !check_groff_preconditions() {
            return Err(PoolError::Message("groff not found on PATH".to_string()));
        }

        let device = match to_ext {
            "html" | "htm" => "html",
            "pdf" => "pdf",
            _ => "utf8",
        };

        let mut child = Command::new("groff")
            .arg("-mandoc")
            .arg(format!("-T{device}"))
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
                "groff conversion failed: {}",
                stderr.trim()
            )));
        }

        Ok(output.stdout)
    }
}

impl EnginePool for GroffPool {
    fn name(&self) -> &'static str {
        "groff"
    }

    fn health(&self) -> PoolResult<PoolHealth> {
        let which = std::process::Command::new("which").arg("groff").output();
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

pub fn check_groff_preconditions() -> bool {
    let out = std::process::Command::new("which").arg("groff").output();
    matches!(out, Ok(o) if o.status.success())
}
