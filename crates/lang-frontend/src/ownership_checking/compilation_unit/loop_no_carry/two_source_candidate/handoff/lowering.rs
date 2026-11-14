//! Only original retained facts; this does not issue ordinary ownership authority.
use super::ExperimentalUnitHandoff;
use crate::{
    ownership_checking::*,
    type_checking::{
        UnitCallableTarget, UnitExpressionId, UnitStatementId, ValidatedCompilationUnitTypes,
    },
};
impl std::fmt::Debug for ExperimentalUnitHandoff<'_, '_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.experiment.retained, formatter)
    }
}
impl UnitLoweringFacts for ExperimentalUnitHandoff<'_, '_> {
    fn is_compatible_with(&self, typed: &ValidatedCompilationUnitTypes) -> bool {
        self.experiment.retained.is_compatible_with(typed)
    }

    fn loan_action_observations(&self) -> &[UnitLoanActionObservation] {
        self.experiment.retained.loan_action_observations()
    }

    fn map_iteration(&self, statement: UnitStatementId) -> Option<&UnitMapIterationOwnershipPlan> {
        self.experiment.retained.map_iteration(statement)
    }

    fn iteration_cleanup_at(&self, point: UnitDropPoint) -> Option<&[UnitIterationCleanupAction]> {
        self.experiment.retained.iteration_cleanup_at(point)
    }

    fn iteration(&self, statement: UnitStatementId) -> Option<&UnitIterationOwnershipPlan> {
        self.experiment.retained.iteration(statement)
    }

    fn bindings(&self) -> &[UnitOwnershipBindingDescriptor] {
        self.experiment.retained.bindings()
    }
    fn borrow_binding_transports(&self) -> &[UnitBorrowBindingTransportFact] {
        self.experiment.retained.borrow_binding_transports()
    }
    fn borrow_call_handoffs(&self) -> &[UnitBorrowCallHandoffFact] {
        self.experiment.retained.borrow_call_handoffs()
    }
    fn call_argument_completed(&self, expression: UnitExpressionId) -> bool {
        self.experiment.retained.call_argument_completed(expression)
    }
    fn call_may_dispatch(&self, call: UnitExpressionId) -> Option<bool> {
        self.experiment.retained.call_may_dispatch(call)
    }
    fn call_site_was_entered(&self, expression: UnitExpressionId) -> bool {
        self.experiment.retained.call_site_was_entered(expression)
    }
    fn callable_origin(&self, expression: UnitExpressionId) -> Option<UnitCallableOriginFact> {
        self.experiment.retained.callable_origin(expression)
    }
    fn captures(&self) -> &[UnitClosureCaptureDescriptor] {
        self.experiment.retained.captures()
    }
    fn closure(&self, expression: UnitExpressionId) -> Option<UnitClosureDescriptor> {
        self.experiment.retained.closure(expression)
    }
    fn closure_callable_return(
        &self,
        target: UnitCallableTarget,
    ) -> Option<&UnitClosureCallableReturnSummary> {
        self.experiment.retained.closure_callable_return(target)
    }
    fn conditional_receiver_deliveries(&self) -> &[UnitConditionalReceiverDeliveryFact] {
        self.experiment.retained.conditional_receiver_deliveries()
    }
    fn conditional_receiver_delivery(
        &self,
        call: UnitExpressionId,
    ) -> Option<UnitConditionalReceiverDeliveryFact> {
        self.experiment.retained.conditional_receiver_delivery(call)
    }
    fn conditional_receiver_drops(&self) -> &[UnitConditionalReceiverDropFact] {
        self.experiment.retained.conditional_receiver_drops()
    }
    fn construction_plans(&self) -> &[UnitConstructionOwnershipPlan] {
        self.experiment.retained.construction_plans()
    }
    fn deferred(&self) -> &[UnitOwnershipDeferredFact] {
        self.experiment.retained.deferred()
    }
    fn delegations(&self) -> &[UnitDelegationOwnershipPlan] {
        self.experiment.retained.delegations()
    }
    fn direct_call_loan_ends(&self) -> &[UnitDirectCallLoanEndFact] {
        self.experiment.retained.direct_call_loan_ends()
    }
    fn drops(&self) -> &[UnitDropFact] {
        self.experiment.retained.drops()
    }
    fn field_replacement(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&super::UnitFieldReplaceOwnershipPlan> {
        self.experiment.retained.field_replacement(expression)
    }
    fn lambda_was_entered(&self, expression: UnitExpressionId) -> bool {
        self.experiment.retained.lambda_was_entered(expression)
    }
    fn non_null_assertion(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&UnitNonNullAssertionOwnershipPlan> {
        self.experiment.retained.non_null_assertion(expression)
    }
    fn nullable_when(&self, control: UnitExpressionId) -> Option<&UnitNullableWhenOwnershipPlan> {
        self.experiment.retained.nullable_when(control)
    }
    fn nullable_when_use(&self, argument: UnitExpressionId) -> Option<&UnitNullableWhenUseFact> {
        self.experiment.retained.nullable_when_use(argument)
    }
    fn owner_observations(&self) -> &[UnitOwnerObservation] {
        self.experiment.retained.owner_observations()
    }
    fn ownership_primitive(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&super::UnitOwnershipPrimitiveOwnershipPlan> {
        self.experiment.retained.ownership_primitive(expression)
    }
    fn pointer_callable_return(
        &self,
        target: UnitCallableTarget,
    ) -> Option<&UnitPointerCallableReturnSummary> {
        self.experiment.retained.pointer_callable_return(target)
    }
    fn rc_effects(&self) -> &[UnitRcOwnershipEffect] {
        self.experiment.retained.rc_effects()
    }
    fn receiver_call_return_ends(&self) -> &[UnitReceiverCallReturnEndFact] {
        self.experiment.retained.receiver_call_return_ends()
    }
    fn receiver_fact(&self, call: UnitExpressionId) -> Option<&UnitReceiverOwnershipFact> {
        self.experiment.retained.receiver_fact(call)
    }
    fn receiver_facts(&self) -> &[UnitReceiverOwnershipFact] {
        self.experiment.retained.receiver_facts()
    }
    fn string_effect(&self, expression: UnitExpressionId) -> Option<UnitStringOwnershipEffect> {
        self.experiment.retained.string_effect(expression)
    }
    fn value_deliveries(&self) -> &[UnitValueDeliveryFact] {
        self.experiment.retained.value_deliveries()
    }
}
