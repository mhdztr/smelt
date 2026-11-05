//! Calibre engine pool (M3).

use crate::error::PoolResult;
use crate::pool::{EnginePool, PoolHealth};

pub struct CalibrePool;

impl EnginePool for CalibrePool {
    fn name(&self) -> &'static str {
        "calibre"
    }

    fn health(&self) -> PoolResult<PoolHealth> {
        // Check if binary exists on PATH
        let which = std::process::Command::new("which").arg("calibre").output();
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

pub fn check_calibre_preconditions() -> bool {
    let out = std::process::Command::new("which").arg("calibre").output();
    matches!(out, Ok(o) if o.status.success())
}
