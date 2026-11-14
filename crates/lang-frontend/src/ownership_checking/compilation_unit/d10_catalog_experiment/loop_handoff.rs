//! Same-run final-owned loop observation. Still not a CFG/lowering capability.
use super::*;
use crate::ownership_checking::UnitLoanFact;
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LoopPending {
    pub(crate) typed: Arc<()>,
    pub(crate) callable: UnitItemId,
    pub(crate) control: UnitStatementId,
    pub(crate) root: UnitSymbolId,
    pub(crate) counter: UnitSymbolId,
    pub(crate) parameter: UnitSymbolId,
    pub(crate) condition: UnitExpressionId,
    pub(crate) step: UnitExpressionId,
    pub(crate) initial: UnitExpressionId,
    pub(crate) incoming: UnitExpressionId,
    pub(crate) call: UnitExpressionId,
    pub(crate) cleanup: [UnitDropFact; 2],
    pub(crate) loan: UnitLoanFact,
}
/// Closed-source loop facts tied to their actual final ownership analysis.
/// This observation has no ordinary codegen admission or public constructor.
pub struct BoundLoop {
    owned: ValidatedCompilationUnitOwnership,
    pending: LoopPending,
}
impl BoundLoop {
    /// Exact final validated ownership instance.
    #[must_use]
    pub const fn ownership(&self) -> &ValidatedCompilationUnitOwnership {
        &self.owned
    }
    /// Full final-ownership identity, not merely shared typed inputs.
    #[must_use]
    pub fn matches(&self, unit: &super::super::super::OwnedCompilationUnitView<'_, '_>) -> bool {
        self.owned.ownership().is_same_analysis(unit.ownership())
    }
    /// Actual callable and loop statement from the closed source envelope.
    #[must_use]
    pub const fn control(&self) -> (UnitItemId, UnitStatementId) {
        (self.pending.callable, self.pending.control)
    }
    /// Actual replaced String root, local Int counter and Shared Int parameter.
    #[must_use]
    pub const fn roots(&self) -> (UnitSymbolId, UnitSymbolId, UnitSymbolId) {
        (
            self.pending.root,
            self.pending.counter,
            self.pending.parameter,
        )
    }
    /// Actual condition and checked counter step; not a CFG edge proof.
    #[must_use]
    pub const fn counter_expressions(&self) -> (UnitExpressionId, UnitExpressionId) {
        (self.pending.condition, self.pending.step)
    }
    /// Initial and per-iteration construction sites, not per-iteration object tokens.
    #[must_use]
    pub const fn producer_sites(&self) -> (UnitExpressionId, UnitExpressionId) {
        (self.pending.initial, self.pending.incoming)
    }
    /// Actual single-root Replace call.
    #[must_use]
    pub const fn replacement(&self) -> UnitExpressionId {
        self.pending.call
    }
    /// Detached-old AfterStatement then current-root LoopExit cleanup obligations.
    #[must_use]
    pub const fn cleanup(&self) -> &[UnitDropFact; 2] {
        &self.pending.cleanup
    }
    /// Actual registered loan; pending removal is not independent proof of loan End.
    #[must_use]
    pub const fn loan(&self) -> &UnitLoanFact {
        &self.pending.loan
    }
}
impl LoopPending {
    fn bind(self, owned: CompilationUnitOwnership) -> Option<BoundLoop> {
        if !Arc::ptr_eq(&self.typed, &owned.provenance.typed_analysis_owner)
            || !owned.diagnostics().is_empty()
            || !owned.deferred().is_empty()
            || owned.drops() != self.cleanup
            || owned.loans() != [self.loan.clone()]
        {
            return None;
        }
        let [plan] = owned.ownership_primitives() else {
            return None;
        };
        if plan.descriptor().expression() != self.call
            || plan.descriptor().kind() != crate::type_checking::OwnershipPrimitiveKind::Replace
            || plan.places().len() != 1
            || !plan.places()[0].is_root()
            || plan.places()[0].root() != self.root
            || plan.descriptor().operands()[1] != self.incoming
        {
            return None;
        }
        Some(BoundLoop {
            owned: owned.validate().ok()?,
            pending: self,
        })
    }
}
/// Runs actual ownership analysis and binds only its complete closed-loop sidecar.
/// No API can retrofit a candidate onto a previously produced ownership product.
pub fn analyze_loop(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
) -> Result<Option<BoundLoop>, OwnershipCheckingError> {
    let (owned, _, mut loops) = analysis::analyze_with_catalog(
        sources,
        inputs,
        names.names(),
        environment,
        typed.types(),
        false,
    )?;
    if inputs.len() != 1 || loops.len() != 1 {
        return Ok(None);
    }
    Ok(loops.pop().and_then(|pending| pending.bind(owned)))
}
