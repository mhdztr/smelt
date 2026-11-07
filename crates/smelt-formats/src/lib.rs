//! Engine adapters and warm pools for external document converters.

pub mod error;
pub mod pool;
pub mod pools;
pub mod soffice;

pub use error::{PoolError, PoolResult};
pub use pool::{EnginePool, PoolHealth};
pub use pools::{
    CalibrePool, GroffPool, PandocPool, TesseractPool, check_calibre_preconditions,
    check_groff_preconditions, check_pandoc_preconditions, check_tesseract_preconditions,
};
pub use soffice::{SofficePool, check_soffice_preconditions};
