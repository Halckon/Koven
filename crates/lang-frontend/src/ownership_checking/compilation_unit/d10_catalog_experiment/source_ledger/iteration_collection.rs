//! Original final cleanup projections, not the entire ActionSource event history.
use super::*;

/// Original cleanup plans and their validation-input source batches. Nested providers may share identical point actions;
/// their plans remain separate and ordered, never flattened into a new execution schedule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationSourceCollection {
    conditional_scopes: Vec<(UnitConditionalReceiverDropFact, Vec<UnitStatementId>)>,
    iterations: Vec<UnitIterationOwnershipPlan>,
    map_iterations: Vec<UnitMapIterationOwnershipPlan>,
    validation: IterationValidationInputs,
}
impl IterationSourceCollection {
    pub(crate) fn capture(
        conditional_scopes: &[(UnitConditionalReceiverDropFact, Vec<UnitStatementId>)],
        iterations: &[UnitIterationOwnershipPlan],
        map_iterations: &[UnitMapIterationOwnershipPlan],
        validation: IterationValidationInputs,
    ) -> Self {
        Self {
            conditional_scopes: conditional_scopes.to_vec(),
            iterations: iterations.to_vec(),
            map_iterations: map_iterations.to_vec(),
            validation,
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
    /// Original owner lexical-scope comparisons, including temporary scopes.
    #[must_use]
    pub fn owner_scopes(&self) -> &[(UnitStatementId, UnitDropTarget, bool)] {
        &self.validation.owner_scopes
    }

    /// Original call, receiver and capture loan-end cleanup projection.
    #[must_use]
    pub fn loan_ends(&self) -> &[(UnitDropPoint, UnitIterationCleanupAction)] {
        &self.validation.loan_ends
    }

    /// Original required exits, retaining provider, exit kind and point.
    #[must_use]
    pub fn required_exits(&self) -> &[(UnitStatementId, UnitIterationExitKind, UnitDropPoint)] {
        &self.validation.required_exits
    }

    /// Original sequential templates in checker map iteration order.
    #[must_use]
    pub fn templates(&self) -> &[UnitIterationOwnershipPlan] {
        &self.validation.templates
    }

    /// Original Map templates in checker map iteration order.
    #[must_use]
    pub fn map_templates(&self) -> &[UnitMapIterationOwnershipPlan] {
        &self.validation.map_templates
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
            && self
                .records
                .iter()
                .filter_map(|record| record.iteration_collection.as_ref())
                .flat_map(|collection| collection.owner_scopes())
                .eq(&owned.iteration_owner_scopes)
            && self
                .records
                .iter()
                .filter_map(|record| record.iteration_collection.as_ref())
                .flat_map(|collection| collection.loan_ends())
                .eq(&owned.iteration_loan_ends)
            && self
                .records
                .iter()
                .filter_map(|record| record.iteration_collection.as_ref())
                .flat_map(|collection| collection.required_exits())
                .eq(&owned.iteration_required_exits)
            && self
                .records
                .iter()
                .filter_map(|record| record.iteration_collection.as_ref())
                .flat_map(|collection| collection.templates())
                .eq(&owned.iteration_templates)
            && self
                .records
                .iter()
                .filter_map(|record| record.iteration_collection.as_ref())
                .flat_map(|collection| collection.map_templates())
                .eq(&owned.map_iteration_templates)
    }
}

/// Same-run validation inputs, not new evidence reconstructed from plans or AST.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IterationValidationInputs {
    owner_scopes: Vec<(UnitStatementId, UnitDropTarget, bool)>,
    loan_ends: Vec<(UnitDropPoint, UnitIterationCleanupAction)>,
    required_exits: Vec<(UnitStatementId, UnitIterationExitKind, UnitDropPoint)>,
    templates: Vec<UnitIterationOwnershipPlan>,
    map_templates: Vec<UnitMapIterationOwnershipPlan>,
}
impl IterationValidationInputs {
    pub(crate) fn capture(
        owner_scopes: &[(UnitStatementId, UnitDropTarget, bool)],
        loan_ends: &[(UnitDropPoint, UnitIterationCleanupAction)],
        required_exits: &[(UnitStatementId, UnitIterationExitKind, UnitDropPoint)],
        templates: Vec<UnitIterationOwnershipPlan>,
        map_templates: Vec<UnitMapIterationOwnershipPlan>,
    ) -> Self {
        Self {
            owner_scopes: owner_scopes.to_vec(),
            loan_ends: loan_ends.to_vec(),
            required_exits: required_exits.to_vec(),
            templates,
            map_templates,
        }
    }
}
