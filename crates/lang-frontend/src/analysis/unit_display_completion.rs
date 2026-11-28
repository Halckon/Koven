//! Re-elaborate implicit formatter bodies from confirmed source-level execution facts.

use crate::{
    name_resolution::{CompilationUnitNames, SourceUnitInput, ValidatedCompilationUnitNames},
    ownership_checking::CompilationUnitOwnership,
    source::SourceMap,
    type_checking::{
        CompilationUnitTypeError, CompilationUnitTypes, CompletedUnitCalls, TypeEnvironment,
        check_types_with_completed_calls,
    },
};

/// Complete canonical body types for implicit Display calls reached by the selected ownership
/// capability. A replacement is a fresh typed product and must undergo admission and ownership
/// again. The provisional ownership product is never valid for that replacement.
pub fn complete_unit_display_types(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Option<CompilationUnitTypes>, CompilationUnitTypeError> {
    complete_display_types(sources, inputs, names.names(), environment, typed, owned)
}

pub(super) fn complete_display_types(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &CompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Option<CompilationUnitTypes>, CompilationUnitTypeError> {
    if !typed.is_compatible_with_raw(sources, inputs, names, environment)
        || !owned.matches_raw_types(typed)
    {
        return Err(CompilationUnitTypeError::MismatchedSignatures);
    }
    if !typed.has_valid_type_facts()
        || !owned.diagnostics().is_empty()
        || !owned.deferred().is_empty()
        || (!typed.has_valid_basic_types() && !owned.has_constant_control_facts())
    {
        return Ok(None);
    }
    let display = owned
        .interpolation_evaluations()
        .iter()
        .filter(|evaluation| evaluation.completes())
        .filter_map(|evaluation| Some((evaluation.operand(), evaluation.requirement()?)))
        .collect::<std::collections::BTreeMap<_, _>>();
    if display.is_empty() {
        return Ok(None);
    }
    let explicit = typed
        .calls()
        .iter()
        .map(|call| call.expression())
        .filter(|&call| {
            owned.call_site_was_entered(call) && owned.call_may_dispatch(call) != Some(false)
        })
        .collect();
    check_types_with_completed_calls(
        sources,
        inputs,
        names,
        environment,
        Some(&CompletedUnitCalls { explicit, display }),
    )
    .map(Some)
}
