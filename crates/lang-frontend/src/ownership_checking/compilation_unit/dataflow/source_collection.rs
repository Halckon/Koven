//! Feature-only source observations at the successful planner return boundary.
//! Production aggregation remains in dataflow; the final ledger compares these complete
//! batches against the original aggregate channels before exposing them.
use super::{
    super::d10_catalog_experiment::{IterationSourceCollection, IterationValidationInputs},
    Checker, drop_planner,
};

pub(super) fn capture_iteration(
    checker: &Checker<'_>,
    analysis: &drop_planner::Analysis,
) -> IterationSourceCollection {
    let validation = IterationValidationInputs::capture(
        &analysis.iteration_owner_scopes,
        &analysis.iteration_loan_ends,
        &analysis.iteration_required_exits,
        // BTreeMap::values preserves exactly the subsequent production into_values order.
        checker.iterations.values().cloned().collect(),
        checker.map_iterations.values().cloned().collect(),
    );
    IterationSourceCollection::capture(
        &analysis.iteration_conditional_scopes,
        &analysis.iterations,
        &analysis.map_iterations,
        validation,
    )
}
