//! Separate two-callable source handoff, never convertible to the old single-callable bound.
use super::*;
use crate::ownership_checking::{UnitDirectCallLoanEndFact, UnitLoanFact};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct WrappedDualPending {
    pub(crate) fields: DualNestedPending,
    pub(crate) contracts: Vec<super::super::UnitCallArgumentOwnershipContract>,
    pub(crate) entry: UnitItemId,
    pub(crate) roots: [UnitSymbolId; 2],
    pub(crate) initializers: [UnitExpressionId; 2],
    pub(crate) call: UnitExpressionId,
    pub(crate) arguments: [UnitExpressionId; 2],
    pub(crate) loans: [UnitLoanFact; 2],
    pub(crate) ends: [UnitDirectCallLoanEndFact; 2],
}

/// Same final ownership analysis for the restricted named wrapper and BeforeChild callee.
/// This is not an executable-entry or general interprocedural ownership capability.
pub struct BoundNamedWrapper {
    owned: ValidatedCompilationUnitOwnership,
    pending: WrappedDualPending,
}
impl BoundNamedWrapper {
    /// Original complete final ownership, not a filtered callee analysis.
    #[must_use]
    pub const fn ownership(&self) -> &ValidatedCompilationUnitOwnership {
        &self.owned
    }
    /// Exact final analysis identity.
    #[must_use]
    pub fn matches(&self, unit: &super::super::super::OwnedCompilationUnitView<'_, '_>) -> bool {
        self.owned.ownership().is_same_analysis(unit.ownership())
    }
    /// Original zero-parameter wrapper declaration.
    #[must_use]
    pub const fn entry(&self) -> UnitItemId {
        self.pending.entry
    }
    /// Read-only callee facts borrowed from this two-function capability, not an old bound.
    #[must_use]
    pub fn callee(&self) -> NamedWrapperCallee<'_> {
        NamedWrapperCallee {
            pending: &self.pending.fields,
        }
    }
    /// Two distinct named argument roots in source argument order.
    #[must_use]
    pub const fn wrapper_roots(&self) -> [UnitSymbolId; 2] {
        self.pending.roots
    }
    /// Original Int literal initializers; not evaluated runtime values.
    #[must_use]
    pub const fn initializers(&self) -> [UnitExpressionId; 2] {
        self.pending.initializers
    }
    /// Actual wrapper invocation.
    #[must_use]
    pub const fn call(&self) -> UnitExpressionId {
        self.pending.call
    }
    /// Actual named argument expressions.
    #[must_use]
    pub const fn arguments(&self) -> [UnitExpressionId; 2] {
        self.pending.arguments
    }
    /// Two actual registrations in source order.
    #[must_use]
    pub const fn loans(&self) -> &[UnitLoanFact; 2] {
        &self.pending.loans
    }
    /// Actual End facts in publication order, second argument then first.
    #[must_use]
    pub const fn ends(&self) -> &[UnitDirectCallLoanEndFact; 2] {
        &self.pending.ends
    }
}

/// Private construction; only readable while its owning two-function bound is borrowed.
pub struct NamedWrapperCallee<'a> {
    pending: &'a DualNestedPending,
}

impl NamedWrapperCallee<'_> {
    /// Actual callable, parent loop and child loop.
    #[must_use]
    pub const fn controls(&self) -> (UnitItemId, UnitStatementId, UnitStatementId) {
        (
            self.pending.callable,
            self.pending.controls[0],
            self.pending.controls[1],
        )
    }
    /// Replaced root, parent counter, child counter, and parent Shared parameter.
    /// Use `counter_parameters` for the child parameter identity.
    #[must_use]
    pub const fn roots(&self) -> (UnitSymbolId, UnitSymbolId, UnitSymbolId, UnitSymbolId) {
        (
            self.pending.root,
            self.pending.counters[0],
            self.pending.counters[1],
            self.pending.counter_parameters[0],
        )
    }
    /// Actual distinct parent/child initializer parameters in this two-parameter form.
    #[must_use]
    pub const fn counter_parameters(&self) -> [UnitSymbolId; 2] {
        self.pending.counter_parameters
    }
    /// Actual parent/child counter initializer expressions, resolved to `counter_parameters`.
    #[must_use]
    pub const fn counter_initializers(&self) -> [UnitExpressionId; 2] {
        self.pending.counter_initializers
    }
    /// Parent then child condition/step. No CFG edge permission.
    #[must_use]
    pub const fn counter_expressions(&self) -> [(UnitExpressionId, UnitExpressionId); 2] {
        [
            (self.pending.conditions[0], self.pending.steps[0]),
            (self.pending.conditions[1], self.pending.steps[1]),
        ]
    }
    /// Parent then child actual Replace expressions.
    #[must_use]
    pub const fn replacements(&self) -> [UnitExpressionId; 2] {
        self.pending.calls
    }
    /// Initial, parent incoming and child incoming source sites; not dynamic identities.
    #[must_use]
    pub const fn producer_sites(&self) -> [UnitExpressionId; 3] {
        self.pending.sites
    }
    /// Parent detached-old, child detached-old and final current-root CallReturn cleanups.
    #[must_use]
    pub const fn cleanup(&self) -> &[UnitDropFact; 3] {
        &self.pending.cleanup
    }
    /// Parent and child registered Exclusive loans. Does not independently prove End.
    #[must_use]
    pub const fn loans(&self) -> &[UnitLoanFact; 2] {
        &self.pending.loans
    }
    /// Explicit actual post-loop use of the replaced root; always Some in this restricted handoff.
    /// Its source site is the initial construction, not the current loop generation.
    /// Only actual CFG exit transport can identify the value consumed by this call.
    #[must_use]
    pub const fn final_root_use(&self) -> Option<&RetainedLoopRoot> {
        self.pending.final_root_use.as_ref()
    }
    /// Required second root with an actual suffix use and cleanup.
    #[must_use]
    pub const fn retained(&self) -> &RetainedLoopRoot {
        &self.pending.retained
    }
}

impl WrappedDualPending {
    fn bind(self, owned: CompilationUnitOwnership) -> Option<BoundNamedWrapper> {
        let f = &self.fields;
        let final_use = f.final_root_use.as_ref()?;
        let drops = [f.cleanup[0], f.cleanup[1], f.cleanup[2], f.retained.cleanup];
        let loans = [
            f.loans[0].clone(),
            f.loans[1].clone(),
            final_use.loan.clone(),
            f.retained.loan.clone(),
            self.loans[0].clone(),
            self.loans[1].clone(),
        ];
        if !Arc::ptr_eq(&f.typed, &owned.provenance.typed_analysis_owner)
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
            super::super::UnitDropTarget::Named(parent_old),
            super::super::UnitDropTarget::Named(child_old),
        ] = [f.cleanup[0].target(), f.cleanup[1].target()]
        else {
            return None;
        };
        let locals = [
            f.root,
            f.retained.root,
            f.counters[0],
            f.counters[1],
            parent_old,
            child_old,
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
        Some(BoundNamedWrapper {
            owned: owned.validate().ok()?,
            pending: self,
        })
    }
}

/// Analyze one original two-function source and bind only after final ownership publication.
/// No API accepts an old bound, caller-supplied trace, filtered facts, or replacement owned result.
pub fn analyze_named_wrapper(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
) -> Result<Option<BoundNamedWrapper>, OwnershipCheckingError> {
    let (owned, _, _, _, _, _, mut candidates) = analysis::analyze_with_catalog(
        sources,
        inputs,
        names.names(),
        environment,
        typed.types(),
        false,
    )?;
    if inputs.len() != 1 || candidates.len() != 1 {
        return Ok(None);
    }
    Ok(candidates.pop().and_then(|pending| pending.bind(owned)))
}
