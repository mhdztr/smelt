//! Pandoc engine pool (M3).

use crate::error::PoolResult;
use crate::pool::{EnginePool, PoolHealth};

pub struct PandocPool;

impl EnginePool for PandocPool {
    fn name(&self) -> &'static str {
        "pandoc"
    }

    fn health(&self) -> PoolResult<PoolHealth> {
        // Check if binary exists on PATH
        let which = std::process::Command::new("which").arg("pandoc").output();
        match which {
            Ok(out) if out.status.success() => Ok(PoolHealth {
                ok: true,
                workers: 0,
                message: Some("binary found".to_string()),
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
