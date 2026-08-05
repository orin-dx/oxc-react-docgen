#![deny(unsafe_code)]

pub(crate) mod cache;
pub(crate) mod extractor;
pub(crate) mod import_map;
pub(crate) mod known;
pub mod panic_guard;
pub mod pipeline;
pub mod react_types;
pub(crate) mod resolver;
pub mod types;

// Re-export the primary consumer API at the crate root.
pub use pipeline::{extract, PipelineOptions};
pub use types::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
pub use types::output::ExtractionOutput;
