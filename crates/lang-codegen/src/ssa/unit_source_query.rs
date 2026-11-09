//! Canonical source lookup shared by the unit planner and lowerer.
use super::{LoweringError, LoweringErrorKind};
use lang_frontend::{
    name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
    parser::ParsedFile,
    type_checking::{CompilationUnitTypes, UnitCallReceiverOrigin, UnitExpressionId},
};

pub(super) fn parsed_by_source_unit<'a>(
    inputs: &'a [SourceUnitInput<'a>],
    names: &ValidatedCompilationUnitNames,
) -> Result<Vec<&'a ParsedFile>, LoweringError> {
    names
        .names()
        .index()
        .source_units()
        .iter()
        .map(|source_unit| {
            inputs
                .iter()
                .copied()
                .find(|input| input.source_id() == source_unit.source_id())
                .map(SourceUnitInput::parsed)
                .ok_or(LoweringError {
                    kind: LoweringErrorKind::MismatchedSource,
                    span: None,
                })
        })
        .collect()
}

/// Return the typed receiver operand only when it cannot reach receiver delivery.
pub(super) fn non_completing_call_receiver(
    typed: &CompilationUnitTypes,
    call: UnitExpressionId,
) -> Option<UnitExpressionId> {
    let UnitCallReceiverOrigin::Expression(receiver) = typed.call(call)?.receiver()?.origin()
    else {
        return None;
    };
    (typed.expression_falls_through(receiver) == Some(false)).then_some(receiver)
}
