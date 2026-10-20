//! Conversion error types. Messages are actionable by design: they say what
//! failed and what to do next (DESIGN.md error rules). The CLI maps them to
//! documented exit codes; they never Debug-dump.

use thiserror::Error;

/// Failure while reading input bytes into an IDM document.
#[derive(Debug, Error)]
pub enum ReadError {
    #[error("input is not valid UTF-8; smelt v0 supports text formats only")]
    NotUtf8,
    #[error("no native reader for {0} in v0; run 'smelt doctor' for available engines")]
    Unsupported(String),
    #[error("could not parse {format}: {message}")]
    Parse { format: String, message: String },
}

/// Failure while writing an IDM document out to bytes.
#[derive(Debug, Error)]
pub enum WriteError {
    #[error("no native writer for {0} in v0; run 'smelt doctor' for available engines")]
    Unsupported(String),
    #[error("could not serialize {format}: {message}")]
    Serialize { format: String, message: String },
}

impl ReadError {
    /// Whether the error means "smelt cannot convert this format at all"
    /// (exit code 3) as opposed to malformed input.
    pub fn is_unsupported(&self) -> bool {
        matches!(self, ReadError::Unsupported(_))
    }
}

impl WriteError {
    /// Whether the error means "smelt cannot produce this format at all"
    /// (exit code 3).
    pub fn is_unsupported(&self) -> bool {
        matches!(self, WriteError::Unsupported(_))
    }
}
