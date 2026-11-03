//! Engine adapters and warm pools for external document converters.

pub mod error;
pub mod pool;
pub mod soffice;

pub use error::{PoolError, PoolResult};
pub use pool::{EnginePool, PoolHealth};
pub use soffice::{SofficePool, check_soffice_preconditions};
