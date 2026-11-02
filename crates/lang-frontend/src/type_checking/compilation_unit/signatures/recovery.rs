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
    collect_signatures_with_files(sources, inputs, unit_names, environment)
        .map(|prepared| prepared.signatures)
}

/// 同次资格检查产生的签名与 canonical AST 借用；不构造新的源码或分析身份。
pub(in crate::type_checking::compilation_unit) struct PreparedUnitSignatures<'a> {
    pub(in crate::type_checking::compilation_unit) signatures: CompilationUnitSignatures,
    pub(in crate::type_checking::compilation_unit) files: Vec<&'a ParsedFile>,
}

pub(in crate::type_checking::compilation_unit) fn collect_signatures_with_files<'a>(
    sources: &'a SourceMap,
    inputs: &[SourceUnitInput<'a>],
    unit_names: &'a CompilationUnitNames,
    environment: &'a TypeEnvironment,
) -> Result<PreparedUnitSignatures<'a>, CompilationUnitTypeError> {
    let rebuilt = index_compilation_unit(sources, inputs)
        .map_err(|_| CompilationUnitTypeError::MismatchedInputs)?;
    if &rebuilt != unit_names.index() {
        return Err(CompilationUnitTypeError::MismatchedInputs);
    }
    // 与 index 的 root/path 键排序一致；先保留原 index 错误优先级，再投影借用。
    let mut ordered_inputs = inputs.to_vec();
    ordered_inputs.sort_by_key(|input| input.key());
    let parsed = unit_names
        .index()
        .source_units()
        .iter()
        .zip(ordered_inputs)
        .map(|(source, input)| {
            if input.source_id() != source.source_id() {
                return Err(CompilationUnitTypeError::MismatchedInputs);
            }
            Ok(input.parsed())
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
    // 只复制 AST 借用向量，collector 消费后原向量继续移交 body；不复制 AST/facts。
    let signatures =
        SignatureCollector::new(sources, parsed.clone(), unit_names, environment)?.run()?;
    Ok(PreparedUnitSignatures {
        signatures,
        files: parsed,
    })
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

#[cfg(test)]
mod alignment_tests;
