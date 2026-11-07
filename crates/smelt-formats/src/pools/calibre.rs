//! Calibre engine pool (M3).

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use crate::error::{PoolError, PoolResult};
use crate::pool::{EnginePool, PoolHealth};

pub struct CalibrePool;

impl Default for CalibrePool {
    fn default() -> Self {
        Self::new()
    }
}

impl CalibrePool {
    pub fn new() -> Self {
        Self
    }

    pub fn convert(&self, input: &[u8], from_ext: &str, to_ext: &str) -> PoolResult<Vec<u8>> {
        if !check_calibre_preconditions() {
            return Err(PoolError::Message(
                "ebook-convert not found on PATH".to_string(),
            ));
        }

        let tmpdir = tempfile::tempdir().map_err(PoolError::Io)?;
        let in_path = tmpdir.path().join(format!("in.{from_ext}"));
        let out_path = tmpdir.path().join(format!("out.{to_ext}"));

        let mut in_file = fs::File::create(&in_path).map_err(PoolError::Io)?;
        in_file.write_all(input).map_err(PoolError::Io)?;

        let mut cmd = Command::new("ebook-convert");
        cmd.arg(&in_path).arg(&out_path);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let output = cmd.output().map_err(PoolError::Io)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PoolError::Message(format!(
                "calibre conversion failed: {}",
                stderr.trim()
            )));
        }

        if !out_path.exists() {
            return Err(PoolError::Message("calibre produced no output".to_string()));
        }

        let data = fs::read(&out_path).map_err(PoolError::Io)?;
        Ok(data)
    }
}

impl EnginePool for CalibrePool {
    fn name(&self) -> &'static str {
        "calibre"
    }

    fn health(&self) -> PoolResult<PoolHealth> {
        if check_calibre_preconditions() {
            Ok(PoolHealth {
                ok: true,
                workers: 1,
                message: Some("ready".to_string()),
            })
        } else {
            Ok(PoolHealth {
                ok: false,
                workers: 0,
                message: Some("ebook-convert not found on PATH".to_string()),
            })
        }
    }

    fn shutdown(&self) {}
}

pub fn check_calibre_preconditions() -> bool {
    if let Ok(out) = std::process::Command::new("which")
        .arg("ebook-convert")
        .output()
        && out.status.success()
    {
        return true;
    }
    let out = std::process::Command::new("which").arg("calibre").output();
    matches!(out, Ok(o) if o.status.success())
}
