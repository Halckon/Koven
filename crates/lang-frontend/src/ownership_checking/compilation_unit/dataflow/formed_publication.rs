//! Final normalization of source-qualified formation history before publication.
use super::{CompilationUnitTypes, Diagnostic, UnitOwnershipDeferredFact, UnitSymbolId};

/// Return only whether this publication gate passed, not overall analysis success.
/// Other facts retain their own cleanup rules at the caller.
pub(super) fn finalize(
    typed: &CompilationUnitTypes,
    diagnostics: &[Diagnostic],
    deferred: &[UnitOwnershipDeferredFact],
    history: &mut Vec<UnitSymbolId>,
) -> bool {
    if !diagnostics.is_empty() || !deferred.is_empty() || !typed.has_valid_type_facts() {
        history.clear();
        false
    } else {
        history.sort();
        history.dedup();
        true
    }
}
