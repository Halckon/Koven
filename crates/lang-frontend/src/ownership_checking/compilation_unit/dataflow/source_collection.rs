//! Feature-only source observations at the successful planner return boundary.
//! Production aggregation remains in dataflow; the final ledger compares these complete
//! batches against the original aggregate channels before exposing them.
use super::{super::d10_catalog_experiment::IterationSourceCollection, drop_planner};

pub(super) fn capture_iteration(analysis: &drop_planner::Analysis) -> IterationSourceCollection {
    IterationSourceCollection::capture(
        &analysis.iteration_conditional_scopes,
        &analysis.iterations,
        &analysis.map_iterations,
    )
}
