//! Generic engine pool traits and types.

use crate::error::PoolResult;

#[derive(Debug, Clone)]
pub struct PoolHealth {
    pub ok: bool,
    pub workers: usize,
    pub message: Option<String>,
}

pub trait EnginePool: Send + Sync {
    fn name(&self) -> &'static str;
    fn health(&self) -> PoolResult<PoolHealth>;
    fn shutdown(&self);
}
