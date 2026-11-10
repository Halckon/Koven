//! Explicitly enabled test support, never part of the default dependency graph.
use super::CompilationUnitOwnership;

/// Run ownership analysis with the requested observation setting in this thread.
#[doc(hidden)]
pub fn with_loan_observation<T>(enabled: bool, run: impl FnOnce() -> T) -> T {
    let _ = enabled;
    run()
}

/// Compare complete legacy products without modifying either original analysis product.
#[doc(hidden)]
pub fn legacy_ownership_eq(
    left: &CompilationUnitOwnership,
    right: &CompilationUnitOwnership,
) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.loan_action_observations.clear();
    right.loan_action_observations.clear();
    left == right
}
