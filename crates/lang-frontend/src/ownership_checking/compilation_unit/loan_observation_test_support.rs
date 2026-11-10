//! Explicitly enabled test support, never part of the default dependency graph.
use super::CompilationUnitOwnership;
use std::cell::Cell;
thread_local! { static ENABLED: Cell<bool> = const { Cell::new(true) }; }

pub(in crate::ownership_checking) fn enabled() -> bool {
    ENABLED.get()
}

/// Run ownership analysis with the requested observation setting in this thread.
#[doc(hidden)]
pub fn with_loan_observation<T>(enabled: bool, run: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            ENABLED.set(self.0);
        }
    }
    let _restore = Restore(ENABLED.replace(enabled));
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
