//! 单 MemoryDocument 的严格 Unit 接入；single 标签保留原 source shape 含义。
use lang_frontend::name_resolution::SymbolKind;

use super::{LoweringError, model::Program};
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;

pub(super) fn single(text: &str) -> Result<Program, LoweringError> {
    let analysis = UnitSource::analyze("result.ko", text)
        .expect("borrow source must pass strict Unit analysis");
    analysis.assert_frontend_clean();
    let entry = analysis.declaration("entry", SymbolKind::Function);
    analysis.lower_entry(entry).map(|(program, _)| program)
}
