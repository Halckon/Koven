//! Independent child-then-parent observation, published only with its final owned result.
//! No dynamic generation tokens, CFG proof, or ordinary codegen admission.
use super::*;
use crate::ownership_checking::UnitLoanFact;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct AfterChildPending {
    pub(crate) typed: Arc<()>,
    pub(crate) callable: UnitItemId,
    pub(crate) controls: [UnitStatementId; 2],
    pub(crate) root: UnitSymbolId,
    pub(crate) counters: [UnitSymbolId; 2],
    pub(crate) counter_parameters: [UnitSymbolId; 2],
    pub(crate) counter_initializers: [UnitExpressionId; 2],
    pub(crate) conditions: [UnitExpressionId; 2],
    pub(crate) steps: [UnitExpressionId; 2],
    pub(crate) sites: [UnitExpressionId; 3],
    pub(crate) calls: [UnitExpressionId; 2],
    pub(crate) cleanup: [UnitDropFact; 3],
    pub(crate) loans: [UnitLoanFact; 2],
    pub(crate) retained: RetainedLoopRoot,
    pub(crate) final_root_use: RetainedLoopRoot,
}
/// Sealed same-run source/fact observation. Not a lowering or Carry capability.
pub struct BoundAfterChildNestedLoop {
    owned: ValidatedCompilationUnitOwnership,
    pending: AfterChildPending,
}
/// Explicit source traversal order, not a dynamic execution count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplacementOrder {
    /// Child traversal completes before the parent replacement is visited.
    ChildThenParent,
}
impl BoundAfterChildNestedLoop {
    /// The sole order admitted by this independent contract.
    #[must_use]
    pub const fn order(&self) -> ReplacementOrder {
        ReplacementOrder::ChildThenParent
    }

    /// The original final validated ownership instance.
    #[must_use]
    pub const fn ownership(&self) -> &ValidatedCompilationUnitOwnership {
        &self.owned
    }
    /// Compares final ownership identities, not just shared typed inputs.
    #[must_use]
    pub fn matches(&self, unit: &super::super::super::OwnedCompilationUnitView<'_, '_>) -> bool {
        self.owned.ownership().is_same_analysis(unit.ownership())
    }
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
    /// Actual distinct parent/child initializer parameters; both are required.
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
    /// Actual traversal order: child then parent Replace expressions.
    #[must_use]
    pub const fn replacements(&self) -> [UnitExpressionId; 2] {
        self.pending.calls
    }
    /// Initial, child incoming and parent incoming source sites; not dynamic identities.
    #[must_use]
    pub const fn producer_sites(&self) -> [UnitExpressionId; 3] {
        self.pending.sites
    }
    /// Actual order: child detached-old, parent detached-old, final current-root cleanups.
    /// The last is always the explicit final_root_use CallReturn.
    #[must_use]
    pub const fn cleanup(&self) -> &[UnitDropFact; 3] {
        &self.pending.cleanup
    }
    /// Child then parent registered Exclusive loans. Does not independently prove End.
    #[must_use]
    pub const fn loans(&self) -> &[UnitLoanFact; 2] {
        &self.pending.loans
    }
    /// Required actual post-loop use of the replaced root.
    /// Its source site is the initial construction, not the current loop generation.
    /// Only actual CFG exit transport can identify the value consumed by this call.
    #[must_use]
    pub const fn final_root_use(&self) -> &RetainedLoopRoot {
        &self.pending.final_root_use
    }
    /// Required second root with an actual suffix use and cleanup.
    #[must_use]
    pub const fn retained(&self) -> &RetainedLoopRoot {
        &self.pending.retained
    }
}
impl AfterChildPending {
    fn bind(self, owned: CompilationUnitOwnership) -> Option<BoundAfterChildNestedLoop> {
        let mut drops = self.cleanup.to_vec();
        drops.push(self.retained.cleanup);
        let mut loans = self.loans.to_vec();
        loans.push(self.final_root_use.loan.clone());
        loans.push(self.retained.loan.clone());
        if !Arc::ptr_eq(&self.typed, &owned.provenance.typed_analysis_owner)
            || !owned.diagnostics().is_empty()
            || !owned.deferred().is_empty()
            || owned.drops() != drops
            || owned.loans() != loans
            || self.controls[0] == self.controls[1]
            || self.calls[0] == self.calls[1]
        {
            return None;
        }
        let plans = owned.ownership_primitives();
        if plans.len() != 2 {
            return None;
        }
        // Preserve actual final-owned primitive order: child then parent, never sort.
        for ((call, incoming), plan) in self
            .calls
            .into_iter()
            .zip(self.sites[1..].iter().copied())
            .zip(plans)
        {
            if plan.descriptor().expression() != call
                || plan.descriptor().kind() != crate::type_checking::OwnershipPrimitiveKind::Replace
                || plan.places().len() != 1
                || !plan.places()[0].is_root()
                || plan.places()[0].root() != self.root
                || plan.descriptor().operands().get(1) != Some(&incoming)
            {
                return None;
            }
        }
        Some(BoundAfterChildNestedLoop {
            owned: owned.validate().ok()?,
            pending: self,
        })
    }
}
/// Analyze once and bind only the complete child-then-parent observation.
/// No public retrofit constructor; failures never return a sidecar.
pub fn analyze_after_child_nested_loop(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
) -> Result<Option<BoundAfterChildNestedLoop>, OwnershipCheckingError> {
    let (owned, _, _, _, _, mut dual, _, _) = analysis::analyze_with_catalog(
        sources,
        inputs,
        names.names(),
        environment,
        typed.types(),
        false,
    )?;
    if inputs.len() != 1 || dual.len() != 1 {
        return Ok(None);
    }
    Ok(dual.pop().and_then(|pending| pending.bind(owned)))
}
