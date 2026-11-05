//! Groff engine pool (M3).

use crate::error::PoolResult;
use crate::pool::{EnginePool, PoolHealth};

pub struct GroffPool;

impl EnginePool for GroffPool {
    fn name(&self) -> &'static str {
        "groff"
    }

    fn health(&self) -> PoolResult<PoolHealth> {
        // Check if binary exists on PATH
        let which = std::process::Command::new("which").arg("groff").output();
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

pub fn check_groff_preconditions() -> bool {
    let out = std::process::Command::new("which").arg("groff").output();
    matches!(out, Ok(o) if o.status.success())
}
