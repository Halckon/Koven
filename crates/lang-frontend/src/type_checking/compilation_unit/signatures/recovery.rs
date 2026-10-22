//! Recovery signature input qualification and parser-proven missing declaration names.
use std::sync::Arc;

use crate::{
    diagnostic::Severity,
    name_resolution::{CompilationUnitNames, SourceUnitInput, index_compilation_unit},
    parser::{NameMarker, ParsedFile},
    source::SourceMap,
    type_checking::TypeEnvironment,
};

use super::{CompilationUnitSignatures, CompilationUnitTypeError, SignatureCollector};

/// 实际 recovery 门复用同一 collector；仍完整核验 source/name/environment 身份。
pub(crate) fn collect_compilation_unit_signatures_recovering(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    unit_names: &CompilationUnitNames,
    environment: &TypeEnvironment,
) -> Result<CompilationUnitSignatures, CompilationUnitTypeError> {
    let rebuilt = index_compilation_unit(sources, inputs)
        .map_err(|_| CompilationUnitTypeError::MismatchedInputs)?;
    if &rebuilt != unit_names.index() {
        return Err(CompilationUnitTypeError::MismatchedInputs);
    }
    let parsed = unit_names
        .index()
        .source_units()
        .iter()
        .map(|source| {
            inputs
                .iter()
                .find(|input| input.source_id() == source.source_id())
                .map(|input| input.parsed())
                .ok_or(CompilationUnitTypeError::MismatchedInputs)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if unit_names.source_units().len() != parsed.len()
        || unit_names.source_units().iter().any(|source| {
            source.resolution().source_id()
                != unit_names.index().source_units()[source.source_unit().index()].source_id()
                || !Arc::ptr_eq(source.resolution().environment_owner(), environment.owner())
        })
    {
        return Err(CompilationUnitTypeError::MismatchedNameEnvironment);
    }
    SignatureCollector::new(sources, parsed, unit_names, environment)?.run()
}

/// Only the parser's actual rejected name permits a declaration without an identity.
/// A missing symbol for a Present name remains an internal consistency failure.
pub(in crate::type_checking::compilation_unit) fn diagnosed_missing_name(
    file: &ParsedFile,
    name: NameMarker,
) -> bool {
    let (NameMarker::Missing(span) | NameMarker::Error(span)) = name else {
        return false;
    };
    file.diagnostics().iter().any(|diagnostic| {
        let primary = diagnostic.primary_span();
        diagnostic.severity() == Severity::Error
            && primary.source_id() == span.source_id()
            && span.start() <= primary.start()
            && primary.end() <= span.end()
    })
}
