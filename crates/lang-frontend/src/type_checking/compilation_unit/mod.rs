//! Compilation-unit 级的规范类型身份与签名收集。

mod bodies;
mod error;
mod iteration;
mod model;
mod shapes;
mod signatures;

pub use bodies::*;
pub use error::CompilationUnitTypeError;
pub use iteration::*;
pub use model::*;
pub use signatures::collect_compilation_unit_signatures;
pub(crate) use signatures::collect_compilation_unit_signatures_recovering;

/// Shared finite specialization budget; Phase 4 retains entry selection and rejection arbitration.
pub const MAX_UNIT_GENERIC_INSTANCES: usize = 1024;
