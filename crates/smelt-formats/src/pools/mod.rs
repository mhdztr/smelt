//! Engine pools for external tools (M3).

pub mod calibre;
pub mod groff;
pub mod pandoc;
pub mod tesseract;

pub use calibre::{CalibrePool, check_calibre_preconditions};
pub use groff::{GroffPool, check_groff_preconditions};
pub use pandoc::{PandocPool, check_pandoc_preconditions};
pub use tesseract::{TesseractPool, check_tesseract_preconditions};
