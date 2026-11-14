//! Order-neutral final-owned comparison. Never creates or converts a source capability.
use super::named_wrapper::WrappedPending;
use super::*;
pub(super) struct NamedValidation<'a> {
    typed: &'a Arc<()>,
    analysis_owner: &'a Arc<()>,
    cleanup: &'a [UnitDropFact; 3],
    loans: &'a [UnitLoanFact; 2],
    retained: &'a RetainedLoopRoot,
    counter_parameters: [UnitSymbolId; 2],
    root: UnitSymbolId,
    calls: [UnitExpressionId; 2],
    sites: [UnitExpressionId; 3],
    counters: [UnitSymbolId; 2],
    final_use: &'a RetainedLoopRoot,
}
impl<'a> NamedValidation<'a> {
    pub(super) fn before(f: &'a DualNestedPending) -> Option<Self> {
        Some(Self {
            typed: &f.typed,
            analysis_owner: &f.analysis_owner,
            cleanup: &f.cleanup,
            loans: &f.loans,
            retained: &f.retained,
            counter_parameters: f.counter_parameters,
            root: f.root,
            calls: f.calls,
            sites: f.sites,
            counters: f.counters,
            final_use: f.final_root_use.as_ref()?,
        })
    }
    pub(super) fn after(f: &'a AfterChildPending) -> Self {
        Self {
            typed: &f.typed,
            analysis_owner: &f.analysis_owner,
            cleanup: &f.cleanup,
            loans: &f.loans,
            retained: &f.retained,
            counter_parameters: f.counter_parameters,
            root: f.root,
            calls: f.calls,
            sites: f.sites,
            counters: f.counters,
            final_use: &f.final_root_use,
        }
    }
}
impl<F> WrappedPending<F> {
    pub(super) fn validate(
        &self,
        owned: CompilationUnitOwnership,
        f: NamedValidation<'_>,
    ) -> Option<ValidatedCompilationUnitOwnership> {
        let final_use = f.final_use;
        let drops = [f.cleanup[0], f.cleanup[1], f.cleanup[2], f.retained.cleanup];
        let callee_loans = [
            f.loans[0].clone(),
            f.loans[1].clone(),
            final_use.loan.clone(),
            f.retained.loan.clone(),
        ];
        let loans: Vec<_> = match self.root_order {
            NamedRootOrder::CalleeFirst => callee_loans.iter().chain(self.loans.iter()),
            NamedRootOrder::EntryFirst => self.loans.iter().chain(callee_loans.iter()),
        }
        .cloned()
        .collect();
        if !Arc::ptr_eq(f.analysis_owner, &owned.provenance.analysis_owner)
            || !Arc::ptr_eq(f.typed, &owned.provenance.typed_analysis_owner)
            || !owned.diagnostics().is_empty()
            || !owned.deferred().is_empty()
            || owned.drops() != drops
            || owned.loans() != loans
            || owned.direct_call_loan_ends() != self.ends
            || f.counter_parameters[0] == f.counter_parameters[1]
            || self.roots[0] == self.roots[1]
            || !owned.call_receiver_contracts().is_empty()
            || !owned.receiver_facts().is_empty()
            || !owned.conditional_receiver_deliveries().is_empty()
            || !owned.conditional_receiver_drops().is_empty()
            || !owned.borrow_call_transports().is_empty()
            || !owned.borrow_binding_transports().is_empty()
            || !owned.borrow_forward_transports().is_empty()
            || !owned.borrow_call_handoffs().is_empty()
            || !owned.borrow_call_prefix_ends().is_empty()
            || !owned.exclusive_call_prefix_ends().is_empty()
            || !owned.receiver_call_prefix_ends().is_empty()
            || !owned.receiver_call_return_ends().is_empty()
            || !owned.string_clone_loan_ends().is_empty()
            || !owned.field_replacements().is_empty()
            || !owned.non_null_assertions().is_empty()
            || !owned.captures().is_empty()
            || !owned.closures().is_empty()
            || !owned.construction_plans().is_empty()
            || !owned.rc_effects().is_empty()
            || !owned.delegations().is_empty()
        {
            return None;
        }
        let plans = owned.ownership_primitives();
        if plans.len() != 2 {
            return None;
        }
        for ((plan, call), incoming) in plans.iter().zip(f.calls).zip(f.sites[1..].iter()) {
            if plan.descriptor().expression() != call
                || plan.descriptor().kind() != crate::type_checking::OwnershipPrimitiveKind::Replace
                || plan.places().len() != 1
                || !plan.places()[0].is_root()
                || plan.places()[0].root() != f.root
                || plan.descriptor().operands().get(1) != Some(incoming)
            {
                return None;
            }
        }
        // Complete source closure limits all calls. Preserve the final array's order; inspect each
        // original contract in place, never build a filtered Ownership for a single-function binder.
        let allowed = [
            f.calls[0],
            f.calls[1],
            final_use.call,
            f.retained.call,
            self.call,
        ];
        if owned.call_argument_contracts().len() != 8
            || owned
                .call_argument_contracts()
                .iter()
                .any(|contract| !allowed.contains(&contract.call()) || contract.crosses_thread())
        {
            return None;
        }
        for (index, argument) in self.arguments.into_iter().enumerate() {
            let mut contracts = owned
                .call_argument_contracts()
                .iter()
                .filter(|c| c.call() == self.call && c.argument() == argument);
            let contract = contracts.next()?;
            if contracts.next().is_some()
                || contract.parameter_index() != index
                || contract.kind() != super::super::UnitCallArgumentOwnershipKind::SharedLoan
                || contract.category() != crate::type_checking::ExpressionCategory::Place
                || contract.argument_span() != self.loans[index].begin_span()
                || contract.call_span() != self.loans[index].end_span()
            {
                return None;
            }
        }
        if owned.call_argument_contracts().iter().any(|actual| {
            self.contracts
                .iter()
                .filter(|expected| *expected == actual)
                .count()
                != 1
        }) || self.contracts.len() != owned.call_argument_contracts().len()
        {
            return None;
        }
        if self.contracts.iter().any(|expected| {
            owned
                .call_argument_contracts()
                .iter()
                .filter(|actual| *actual == expected)
                .count()
                != 1
        }) {
            return None;
        }
        // The public bindings array contains parameters, not local val/var descriptors.
        if owned.bindings().len() != 2
            || f.counter_parameters.iter().any(|parameter| {
                owned.binding_kind(*parameter) != Some(super::super::OwnershipBindingKind::Shared)
            })
        {
            return None;
        }
        let [
            super::super::UnitDropTarget::Named(first_old),
            super::super::UnitDropTarget::Named(second_old),
        ] = [f.cleanup[0].target(), f.cleanup[1].target()]
        else {
            return None;
        };
        let locals = [
            f.root,
            f.retained.root,
            f.counters[0],
            f.counters[1],
            first_old,
            second_old,
            self.roots[0],
            self.roots[1],
        ];
        if locals
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != locals.len()
            || owned.formed_owned_locals().len() != locals.len()
            || locals.iter().any(|root| {
                owned
                    .formed_owned_locals()
                    .iter()
                    .filter(|id| *id == root)
                    .count()
                    != 1
            })
        {
            return None;
        }
        // Remaining validated value-delivery/owner-observation facts retain their ordinary meaning;
        // this source handoff does not claim a new dynamic owner-state proof.
        owned.validate().ok()
    }
}
