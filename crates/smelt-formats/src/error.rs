use thiserror::Error;

#[derive(Debug, Error)]
pub enum PoolError {
    #[error("{0}")]
    Message(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type PoolResult<T> = Result<T, PoolError>;
