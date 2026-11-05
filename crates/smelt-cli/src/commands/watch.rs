//! Watch command (M3).

use crate::SmeltError;
use crate::cli::WatchArgs;

pub fn run(_args: &WatchArgs) -> Result<(), SmeltError> {
    Err(SmeltError::usage(
        "watch requires filesystem notifications (not implemented in this build)",
    ))
}
