//! Sealed, read-only original facts shared by Unit planning and body emission.
//! A query provider is not source identity, a plan, or permission to publish a Program.
use super::*;
use crate::type_checking::{
    UnitCallableTarget, UnitExpressionId, UnitStatementId, ValidatedCompilationUnitTypes,
};

/// Frontend-issued facts only. No raw ownership, conversion or callback escape.
/// ```compile_fail,E0277
/// use lang_frontend::ownership_checking::UnitLoweringFacts;
/// struct Forged;
/// impl UnitLoweringFacts for Forged {}
/// ```
/// Query access cannot be converted back into ordinary ownership authority.
/// ```compile_fail,E0599
/// use lang_frontend::ownership_checking::{CompilationUnitOwnership, UnitLoweringFacts};
/// fn escape(facts: &dyn UnitLoweringFacts) -> &CompilationUnitOwnership {
///     facts.ownership()
/// }
/// ```
pub trait UnitLoweringFacts: UnitPreflightFacts + std::fmt::Debug {
    /// Original is_compatible_with query.
    fn is_compatible_with(&self, typed: &ValidatedCompilationUnitTypes) -> bool;
    /// Original loan_action_observations query.
    fn loan_action_observations(&self) -> &[UnitLoanActionObservation];
    /// Original map_iteration query.
    fn map_iteration(&self, statement: UnitStatementId) -> Option<&UnitMapIterationOwnershipPlan>;
    /// Original iteration_cleanup_at query.
    fn iteration_cleanup_at(&self, point: UnitDropPoint) -> Option<&[UnitIterationCleanupAction]>;
    /// Original iteration query.
    fn iteration(&self, statement: UnitStatementId) -> Option<&UnitIterationOwnershipPlan>;
    /// Original bindings query.
    fn bindings(&self) -> &[UnitOwnershipBindingDescriptor];
    /// Original borrow binding transports query.
    fn borrow_binding_transports(&self) -> &[UnitBorrowBindingTransportFact];
    /// Original borrow call handoffs query.
    fn borrow_call_handoffs(&self) -> &[UnitBorrowCallHandoffFact];
    /// Original call argument completed query.
    fn call_argument_completed(&self, expression: UnitExpressionId) -> bool;
    /// Original call may dispatch query.
    fn call_may_dispatch(&self, call: UnitExpressionId) -> Option<bool>;
    /// Original call site was entered query.
    fn call_site_was_entered(&self, expression: UnitExpressionId) -> bool;
    /// Original callable origin query.
    fn callable_origin(&self, expression: UnitExpressionId) -> Option<UnitCallableOriginFact>;
    /// Original captures query.
    fn captures(&self) -> &[UnitClosureCaptureDescriptor];
    /// Original closure query.
    fn closure(&self, expression: UnitExpressionId) -> Option<UnitClosureDescriptor>;
    /// Original closure callable return query.
    fn closure_callable_return(
        &self,
        target: UnitCallableTarget,
    ) -> Option<&UnitClosureCallableReturnSummary>;
    /// Original conditional receiver deliveries query.
    fn conditional_receiver_deliveries(&self) -> &[UnitConditionalReceiverDeliveryFact];
    /// Original conditional receiver delivery query.
    fn conditional_receiver_delivery(
        &self,
        call: UnitExpressionId,
    ) -> Option<UnitConditionalReceiverDeliveryFact>;
    /// Original conditional receiver drops query.
    fn conditional_receiver_drops(&self) -> &[UnitConditionalReceiverDropFact];
    /// Original construction plans query.
    fn construction_plans(&self) -> &[UnitConstructionOwnershipPlan];
    /// Original deferred query.
    fn deferred(&self) -> &[UnitOwnershipDeferredFact];
    /// Original delegations query.
    fn delegations(&self) -> &[UnitDelegationOwnershipPlan];
    /// Original direct call loan ends query.
    fn direct_call_loan_ends(&self) -> &[UnitDirectCallLoanEndFact];
    /// Original drops query.
    fn drops(&self) -> &[UnitDropFact];
    /// Original field replacement query.
    fn field_replacement(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&super::UnitFieldReplaceOwnershipPlan>;
    /// Original lambda was entered query.
    fn lambda_was_entered(&self, expression: UnitExpressionId) -> bool;
    /// Original non null assertion query.
    fn non_null_assertion(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&UnitNonNullAssertionOwnershipPlan>;
    /// Original nullable when query.
    fn nullable_when(&self, control: UnitExpressionId) -> Option<&UnitNullableWhenOwnershipPlan>;
    /// Original nullable when use query.
    fn nullable_when_use(&self, argument: UnitExpressionId) -> Option<&UnitNullableWhenUseFact>;
    /// Original owner observations query.
    fn owner_observations(&self) -> &[UnitOwnerObservation];
    /// Original ownership primitive query.
    fn ownership_primitive(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&super::UnitOwnershipPrimitiveOwnershipPlan>;
    /// Original pointer callable return query.
    fn pointer_callable_return(
        &self,
        target: UnitCallableTarget,
    ) -> Option<&UnitPointerCallableReturnSummary>;
    /// Original rc effects query.
    fn rc_effects(&self) -> &[UnitRcOwnershipEffect];
    /// Original receiver call return ends query.
    fn receiver_call_return_ends(&self) -> &[UnitReceiverCallReturnEndFact];
    /// Original receiver fact query.
    fn receiver_fact(&self, call: UnitExpressionId) -> Option<&UnitReceiverOwnershipFact>;
    /// Original receiver facts query.
    fn receiver_facts(&self) -> &[UnitReceiverOwnershipFact];
    /// Original string effect query.
    fn string_effect(&self, expression: UnitExpressionId) -> Option<UnitStringOwnershipEffect>;
    /// Original value deliveries query.
    fn value_deliveries(&self) -> &[UnitValueDeliveryFact];
}

impl UnitLoweringFacts for CompilationUnitOwnership {
    fn is_compatible_with(&self, typed: &ValidatedCompilationUnitTypes) -> bool {
        CompilationUnitOwnership::is_compatible_with(self, typed)
    }

    fn loan_action_observations(&self) -> &[UnitLoanActionObservation] {
        CompilationUnitOwnership::loan_action_observations(self)
    }

    fn map_iteration(&self, statement: UnitStatementId) -> Option<&UnitMapIterationOwnershipPlan> {
        CompilationUnitOwnership::map_iteration(self, statement)
    }

    fn iteration_cleanup_at(&self, point: UnitDropPoint) -> Option<&[UnitIterationCleanupAction]> {
        CompilationUnitOwnership::iteration_cleanup_at(self, point)
    }

    fn iteration(&self, statement: UnitStatementId) -> Option<&UnitIterationOwnershipPlan> {
        CompilationUnitOwnership::iteration(self, statement)
    }

    fn bindings(&self) -> &[UnitOwnershipBindingDescriptor] {
        CompilationUnitOwnership::bindings(self)
    }
    fn borrow_binding_transports(&self) -> &[UnitBorrowBindingTransportFact] {
        CompilationUnitOwnership::borrow_binding_transports(self)
    }
    fn borrow_call_handoffs(&self) -> &[UnitBorrowCallHandoffFact] {
        CompilationUnitOwnership::borrow_call_handoffs(self)
    }
    fn call_argument_completed(&self, expression: UnitExpressionId) -> bool {
        CompilationUnitOwnership::call_argument_completed(self, expression)
    }
    fn call_may_dispatch(&self, call: UnitExpressionId) -> Option<bool> {
        CompilationUnitOwnership::call_may_dispatch(self, call)
    }
    fn call_site_was_entered(&self, expression: UnitExpressionId) -> bool {
        CompilationUnitOwnership::call_site_was_entered(self, expression)
    }
    fn callable_origin(&self, expression: UnitExpressionId) -> Option<UnitCallableOriginFact> {
        CompilationUnitOwnership::callable_origin(self, expression)
    }
    fn captures(&self) -> &[UnitClosureCaptureDescriptor] {
        CompilationUnitOwnership::captures(self)
    }
    fn closure(&self, expression: UnitExpressionId) -> Option<UnitClosureDescriptor> {
        CompilationUnitOwnership::closure(self, expression)
    }
    fn closure_callable_return(
        &self,
        target: UnitCallableTarget,
    ) -> Option<&UnitClosureCallableReturnSummary> {
        CompilationUnitOwnership::closure_callable_return(self, target)
    }
    fn conditional_receiver_deliveries(&self) -> &[UnitConditionalReceiverDeliveryFact] {
        CompilationUnitOwnership::conditional_receiver_deliveries(self)
    }
    fn conditional_receiver_delivery(
        &self,
        call: UnitExpressionId,
    ) -> Option<UnitConditionalReceiverDeliveryFact> {
        CompilationUnitOwnership::conditional_receiver_delivery(self, call)
    }
    fn conditional_receiver_drops(&self) -> &[UnitConditionalReceiverDropFact] {
        CompilationUnitOwnership::conditional_receiver_drops(self)
    }
    fn construction_plans(&self) -> &[UnitConstructionOwnershipPlan] {
        CompilationUnitOwnership::construction_plans(self)
    }
    fn deferred(&self) -> &[UnitOwnershipDeferredFact] {
        CompilationUnitOwnership::deferred(self)
    }
    fn delegations(&self) -> &[UnitDelegationOwnershipPlan] {
        CompilationUnitOwnership::delegations(self)
    }
    fn direct_call_loan_ends(&self) -> &[UnitDirectCallLoanEndFact] {
        CompilationUnitOwnership::direct_call_loan_ends(self)
    }
    fn drops(&self) -> &[UnitDropFact] {
        CompilationUnitOwnership::drops(self)
    }
    fn field_replacement(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&super::UnitFieldReplaceOwnershipPlan> {
        CompilationUnitOwnership::field_replacement(self, expression)
    }
    fn lambda_was_entered(&self, expression: UnitExpressionId) -> bool {
        CompilationUnitOwnership::lambda_was_entered(self, expression)
    }
    fn non_null_assertion(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&UnitNonNullAssertionOwnershipPlan> {
        CompilationUnitOwnership::non_null_assertion(self, expression)
    }
    fn nullable_when(&self, control: UnitExpressionId) -> Option<&UnitNullableWhenOwnershipPlan> {
        CompilationUnitOwnership::nullable_when(self, control)
    }
    fn nullable_when_use(&self, argument: UnitExpressionId) -> Option<&UnitNullableWhenUseFact> {
        CompilationUnitOwnership::nullable_when_use(self, argument)
    }
    fn owner_observations(&self) -> &[UnitOwnerObservation] {
        CompilationUnitOwnership::owner_observations(self)
    }
    fn ownership_primitive(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&super::UnitOwnershipPrimitiveOwnershipPlan> {
        CompilationUnitOwnership::ownership_primitive(self, expression)
    }
    fn pointer_callable_return(
        &self,
        target: UnitCallableTarget,
    ) -> Option<&UnitPointerCallableReturnSummary> {
        CompilationUnitOwnership::pointer_callable_return(self, target)
    }
    fn rc_effects(&self) -> &[UnitRcOwnershipEffect] {
        CompilationUnitOwnership::rc_effects(self)
    }
    fn receiver_call_return_ends(&self) -> &[UnitReceiverCallReturnEndFact] {
        CompilationUnitOwnership::receiver_call_return_ends(self)
    }
    fn receiver_fact(&self, call: UnitExpressionId) -> Option<&UnitReceiverOwnershipFact> {
        CompilationUnitOwnership::receiver_fact(self, call)
    }
    fn receiver_facts(&self) -> &[UnitReceiverOwnershipFact] {
        CompilationUnitOwnership::receiver_facts(self)
    }
    fn string_effect(&self, expression: UnitExpressionId) -> Option<UnitStringOwnershipEffect> {
        CompilationUnitOwnership::string_effect(self, expression)
    }
    fn value_deliveries(&self) -> &[UnitValueDeliveryFact] {
        CompilationUnitOwnership::value_deliveries(self)
    }
}

// Inherent dyn helpers retain the original stable filter order without boxing iterators.
impl dyn UnitLoweringFacts + '_ {
    /// Contracts for one original call, in source argument order.
    pub fn call_argument_contracts_for(
        &self,
        call: UnitExpressionId,
    ) -> impl Iterator<Item = &UnitCallArgumentOwnershipContract> {
        self.call_argument_contracts()
            .iter()
            .filter(move |contract| contract.call() == call)
    }
    /// Captures for one original lambda, in source order.
    pub fn captures_of(
        &self,
        lambda: UnitExpressionId,
    ) -> impl Iterator<Item = &UnitClosureCaptureDescriptor> {
        self.captures()
            .iter()
            .filter(move |capture| capture.lambda() == lambda)
    }
}
