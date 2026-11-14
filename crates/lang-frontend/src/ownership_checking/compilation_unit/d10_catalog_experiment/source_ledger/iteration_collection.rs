//! Original final cleanup projections, not the entire ActionSource event history.
use super::*;

/// Three original source-batch channels. Nested providers may share identical point actions;
/// their plans remain separate and ordered, never flattened into a new execution schedule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationSourceCollection {
    conditional_scopes: Vec<(UnitConditionalReceiverDropFact, Vec<UnitStatementId>)>,
    iterations: Vec<UnitIterationOwnershipPlan>,
    map_iterations: Vec<UnitMapIterationOwnershipPlan>,
}
impl IterationSourceCollection {
    pub(crate) fn capture(
        conditional_scopes: &[(UnitConditionalReceiverDropFact, Vec<UnitStatementId>)],
        iterations: &[UnitIterationOwnershipPlan],
        map_iterations: &[UnitMapIterationOwnershipPlan],
    ) -> Self {
        Self {
            conditional_scopes: conditional_scopes.to_vec(),
            iterations: iterations.to_vec(),
            map_iterations: map_iterations.to_vec(),
        }
    }
    /// Original conditional receiver formation scopes, not inferred from a flat Drop ordinal.
    #[must_use]
    pub fn conditional_scopes(&self) -> &[(UnitConditionalReceiverDropFact, Vec<UnitStatementId>)] {
        &self.conditional_scopes
    }
    /// Complete sequential provider plans, each retaining its original ordered exit/actions.
    #[must_use]
    pub fn iterations(&self) -> &[UnitIterationOwnershipPlan] {
        &self.iterations
    }
    /// Complete Map provider plans; not reinterpreted as sequential element providers.
    #[must_use]
    pub fn map_iterations(&self) -> &[UnitMapIterationOwnershipPlan] {
        &self.map_iterations
    }
}
impl SourceCollectionLedger {
    pub(super) fn iteration_collections_match(&self, owned: &CompilationUnitOwnership) -> bool {
        self.records
            .iter()
            .all(|record| record.iteration_collection.is_some())
            && self
                .records
                .iter()
                .filter_map(|record| record.iteration_collection.as_ref())
                .flat_map(|collection| &collection.conditional_scopes)
                .eq(&owned.iteration_conditional_scopes)
            && self
                .records
                .iter()
                .filter_map(|record| record.iteration_collection.as_ref())
                .flat_map(|collection| &collection.iterations)
                .eq(owned.iterations())
            && self
                .records
                .iter()
                .filter_map(|record| record.iteration_collection.as_ref())
                .flat_map(|collection| &collection.map_iterations)
                .eq(owned.map_iterations())
    }
}
