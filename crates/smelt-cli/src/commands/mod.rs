//! Command handlers: one module per subcommand (docs/cli.md).

pub mod completions;
pub mod convert;
pub mod doctor;
pub mod info;

use std::path::Path;

use crate::SmeltError;

/// Map an I/O failure on an input path to a documented exit code
/// (docs/cli.md). Only `NotFound` is "missing input" (exit 4); a permission
/// error is actionable in its own right and must not claim the file does not
/// exist, so it reports the OS reason at exit 1. Shared by `convert` and
/// `info` so both commands classify the same failure the same way.
pub(crate) fn io_error(err: &std::io::Error, path: &Path) -> SmeltError {
    if err.kind() == std::io::ErrorKind::NotFound {
        SmeltError::missing(format!("no such file: {}", path.display()))
    } else {
        SmeltError::new(
            1,
            format!("cannot read {}: {}", path.display(), io_reason(err)),
        )
    }
}

/// A short, human-readable reason for an I/O error (no Debug dump).
pub(crate) fn io_reason(err: &std::io::Error) -> String {
    match err.kind() {
        std::io::ErrorKind::PermissionDenied => "permission denied".to_owned(),
        std::io::ErrorKind::IsADirectory => "path is a directory".to_owned(),
        _ => err.to_string(),
    }
}
