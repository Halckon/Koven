//! 普通 owned-unit 的受控兼容入口与已封闭交接；共享 raw-facts driver 保持私有。

use lang_frontend::{
    name_resolution::{DeclarationId, SourceUnitInput, ValidatedCompilationUnitNames},
    ownership_checking::{
        OwnedCompilationUnitView, OwnedCompilationUnitViewError, ValidatedCompilationUnitOwnership,
        owned_compilation_unit_view,
    },
    source::SourceMap,
    type_checking::{TypeEnvironment, ValidatedCompilationUnitTypes},
};

use super::{FunctionId, LoweringError, LoweringErrorKind, Program, lower_unit_from_facts};

impl From<OwnedCompilationUnitViewError> for LoweringError {
    fn from(error: OwnedCompilationUnitViewError) -> Self {
        Self {
            kind: match error {
                OwnedCompilationUnitViewError::MismatchedSource => {
                    LoweringErrorKind::MismatchedSource
                }
                OwnedCompilationUnitViewError::MismatchedAnalysis => {
                    LoweringErrorKind::MismatchedAnalysis
                }
            },
            span: None,
        }
    }
}

/// 保留既有 scalar unit adapter；一次 factory 校验后进入封闭交接。
pub(crate) fn lower_scalar_unit_with_entry(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
    owned: &ValidatedCompilationUnitOwnership,
    entry: DeclarationId,
) -> Result<(Program, FunctionId), LoweringError> {
    let unit = owned_compilation_unit_view(sources, inputs, names, environment, typed, owned)?;
    lower_owned_unit_with_entry(&unit, entry)
}

/// 已封闭的普通 unit 不再重建来源 index。
pub(crate) fn lower_owned_unit_with_entry(
    unit: &OwnedCompilationUnitView<'_, '_>,
    entry: DeclarationId,
) -> Result<(Program, FunctionId), LoweringError> {
    lower_unit_from_facts(
        unit.sources(),
        unit.inputs(),
        unit.names(),
        unit.types(),
        unit.ownership(),
        None,
        entry,
    )
}
