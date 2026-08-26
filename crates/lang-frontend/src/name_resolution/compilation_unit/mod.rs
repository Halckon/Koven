//! compilation unit 的稳定输入、身份索引与分阶段门禁。

mod diagnostic_order;
mod index;
mod model;

pub use diagnostic_order::{UnitDiagnosticOrderError, ordered_unit_diagnostics};
pub use index::{CompilationUnitInputError, LogicalPathError, index_compilation_unit};
pub use model::*;
