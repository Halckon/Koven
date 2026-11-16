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

// Own source slicing and sealing together, preserving the original aggregate append order.
type Channels<'a> = (
    &'a [super::UnitLoanFact],
    &'a [super::UnitReceiverOwnershipFact],
    &'a [super::UnitConstructionOwnershipPlan],
    &'a [super::UnitDropFact],
    &'a [super::UnitConditionalReceiverDropFact],
);
pub(super) fn capture_source(
    source: crate::name_resolution::SourceUnitId,
    start: (usize, usize, usize, usize, usize),
    channels: Channels<'_>,
    drops: super::super::d10_catalog_experiment::DropCollections,
    iteration: IterationSourceCollection,
    producers: Option<Vec<Option<super::super::d10_catalog_experiment::LoanProducer>>>,
) -> super::super::d10_catalog_experiment::SourceCollectionRecord {
    super::super::d10_catalog_experiment::SourceCollectionRecord::capture(
        source,
        &channels.0[start.0..],
        &channels.1[start.1..],
        &channels.2[start.2..],
        &channels.3[start.3..],
        drops,
        &channels.4[start.4..],
    )
    .with_iteration_collection(iteration)
    .with_loan_producers(producers)
}
