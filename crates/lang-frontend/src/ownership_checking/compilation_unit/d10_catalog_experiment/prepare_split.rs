//! Explicit complete-Unit admission, one ownership run, and no fallback or old-bound conversion.
use super::*;
use crate::{
    name_resolution::{SymbolKind, UnitDeclaration},
    type_checking::complete_compilation_unit_types,
};
/// Original final-admitted types and same-run complete split-source observation.
/// No mutable access, public constructor, or conversion to ordinary/codegen permissions.
pub struct PreparedSplitNamedEntry {
    typed: ValidatedCompilationUnitTypes,
    bound: BoundSplitNamedEntry,
}
impl PreparedSplitNamedEntry {
    /// Original types admitted before the sole ownership analysis.
    #[must_use]
    pub const fn typed(&self) -> &ValidatedCompilationUnitTypes {
        &self.typed
    }
    /// Original complete ownership observation from that same analysis.
    #[must_use]
    pub const fn bound(&self) -> &BoundSplitNamedEntry {
        &self.bound
    }
}
/// Complete final types and ownership before checking the closed two-source scope.
/// Original language failures take precedence over scope/selection refusals. No constant
/// rewriting, source cropping, second ownership run, or ordinary lowering fallback occurs.
pub fn prepare_split_named_entry(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: ValidatedCompilationUnitTypes,
) -> Result<PreparedSplitNamedEntry, PrepareNamedEntryError> {
    complete_compilation_unit_types(sources, inputs, names, environment, typed.types())
        .map_err(PrepareNamedEntryError::FinalTypeAdmission)?;
    let (owned, _, _, _, _, _, _, _, pending) = analysis::analyze_with_catalog(
        sources,
        inputs,
        names.names(),
        environment,
        typed.types(),
        false,
    )
    .map_err(PrepareNamedEntryError::Ownership)?;
    if !owned.diagnostics().is_empty() || !owned.deferred().is_empty() {
        return Err(PrepareNamedEntryError::OwnershipAdmission {
            diagnostics: owned.diagnostics().to_vec(),
            deferred: owned.deferred().to_vec(),
        });
    }
    let bound = super::split_source::select(sources, inputs, names, owned, pending)?;
    Ok(PreparedSplitNamedEntry { typed, bound })
}
/// Prepare once, then select only a declaration borrowed from the original names instance.
/// A value-equal clone or foreign declaration is not authority; admission failures win first.
pub fn prepare_selected_split_named_entry(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: ValidatedCompilationUnitTypes,
    entry: &UnitDeclaration,
) -> Result<PreparedSplitNamedEntry, PrepareNamedEntryError> {
    let prepared = prepare_split_named_entry(sources, inputs, names, environment, typed)?;
    if !names
        .names()
        .index()
        .declarations()
        .iter()
        .any(|d| std::ptr::eq(d, entry))
    {
        return Err(PrepareNamedEntryError::ForeignEntry);
    }
    let bound = prepared.bound();
    if entry.kind() != SymbolKind::Function
        || entry.source_unit() != bound.entry().source_unit()
        || entry.root() != bound.entry().item()
    {
        return Err(PrepareNamedEntryError::EntryMismatch);
    }
    Ok(prepared)
}
