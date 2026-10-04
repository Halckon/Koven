//! Canonical source lookup shared by the unit planner and lowerer.
use super::{LoweringError, LoweringErrorKind};
use lang_frontend::{
    name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
    parser::ParsedFile,
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
