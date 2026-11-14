//! Separate two-callable source handoff, never convertible to the old single-callable bound.
use super::*;
use crate::ownership_checking::{UnitDirectCallLoanEndFact, UnitLoanFact};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct WrappedPending<F> {
    pub(crate) fields: F,
    pub(crate) contracts: Vec<super::super::UnitCallArgumentOwnershipContract>,
    pub(crate) entry: UnitItemId,
    pub(crate) roots: [UnitSymbolId; 2],
    pub(crate) initializers: [UnitExpressionId; 2],
    pub(crate) call: UnitExpressionId,
    pub(crate) arguments: [UnitExpressionId; 2],
    pub(crate) loans: [UnitLoanFact; 2],
    pub(crate) ends: [UnitDirectCallLoanEndFact; 2],
}

pub(crate) type WrappedDualPending = WrappedPending<DualNestedPending>;
pub(crate) type WrappedAfterChildPending = WrappedPending<AfterChildPending>;

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
        let owned = self.validate(
            owned,
            super::named_validation::NamedValidation::before(&self.fields)?,
        )?;
        Some(BoundNamedWrapper {
            owned,
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
    let (owned, _, _, _, _, _, mut candidates, _) = analysis::analyze_with_catalog(
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
