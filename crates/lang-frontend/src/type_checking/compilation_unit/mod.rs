//! Compilation-unit 级的规范类型身份与签名收集。

mod error;
mod model;
mod shapes;
mod signatures;

pub use error::CompilationUnitTypeError;
pub use model::*;
pub use signatures::collect_compilation_unit_signatures;
