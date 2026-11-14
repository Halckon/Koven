//! Closed nested observations bound only after their original owned analysis validates.
//! Source closure and observed facts are not dynamic CFG or Carry permission.
use super::*;
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct NestedPending {
    // Reuse only the exact final fact/identity binder, not singleton admission.
    pub(crate) facts: LoopPending,
    pub(crate) replacement_control: UnitStatementId,
    pub(crate) child: UnitStatementId,
    pub(crate) child_counter: UnitSymbolId,
    pub(crate) child_condition: UnitExpressionId,
    pub(crate) child_step: UnitExpressionId,
}
/// Independent sealed nested sidecar. Ordinary lowering accepts no such capability.
pub struct BoundNestedLoop {
    facts: BoundLoop,
    replacement_control: UnitStatementId,
    child: UnitStatementId,
    child_counter: UnitSymbolId,
    child_condition: UnitExpressionId,
    child_step: UnitExpressionId,
}
impl BoundNestedLoop {
    /// Exact final ownership instance, not a second traversal.
    #[must_use]
    pub const fn ownership(&self) -> &ValidatedCompilationUnitOwnership {
        self.facts.ownership()
    }
    /// Exact analysis identity.
    #[must_use]
    pub fn matches(&self, unit: &super::super::super::OwnedCompilationUnitView<'_, '_>) -> bool {
        self.facts.matches(unit)
    }
    /// Callable, parent loop, child loop. No CFG edge authority.
    #[must_use]
    pub const fn controls(&self) -> (UnitItemId, UnitStatementId, UnitStatementId) {
        let (callable, parent) = self.facts.control();
        (callable, parent, self.child)
    }
    /// Replaced root, parent counter, child counter, Shared parameter.
    #[must_use]
    pub const fn roots(&self) -> (UnitSymbolId, UnitSymbolId, UnitSymbolId, UnitSymbolId) {
        let (root, parent, parameter) = self.facts.roots();
        (root, parent, self.child_counter, parameter)
    }
    /// Parent condition/step then child condition/step.
    #[must_use]
    pub const fn counter_expressions(&self) -> [(UnitExpressionId, UnitExpressionId); 2] {
        [
            self.facts.counter_expressions(),
            (self.child_condition, self.child_step),
        ]
    }
    /// Initial and Replace-body incoming sites, not dynamic object tokens.
    #[must_use]
    pub const fn producer_sites(&self) -> (UnitExpressionId, UnitExpressionId) {
        self.facts.producer_sites()
    }
    /// Actual loop whose body contains the one Replace, distinct from cleanup control.
    #[must_use]
    pub const fn replacement_control(&self) -> UnitStatementId {
        self.replacement_control
    }
    /// Actual Replace expression.
    #[must_use]
    pub const fn replacement(&self) -> UnitExpressionId {
        self.facts.replacement()
    }
    /// Detached old cleanup then parent LoopExit root cleanup.
    #[must_use]
    pub const fn cleanup(&self) -> &[UnitDropFact; 2] {
        self.facts.cleanup()
    }
    /// Actual exclusive root loan; not inferred End.
    #[must_use]
    pub const fn loan(&self) -> &crate::ownership_checking::UnitLoanFact {
        self.facts.loan()
    }
    /// Required second root with genuine suffix use.
    #[must_use]
    pub const fn retained(&self) -> Option<&RetainedLoopRoot> {
        self.facts.retained()
    }
}
impl NestedPending {
    fn bind(self, owned: CompilationUnitOwnership) -> Option<BoundNestedLoop> {
        if ![self.child, self.facts.control].contains(&self.replacement_control)
            || self.facts.retained.is_none()
            || self.child == self.facts.control
            || [self.facts.root, self.facts.counter, self.facts.parameter]
                .contains(&self.child_counter)
        {
            return None;
        }
        Some(BoundNestedLoop {
            replacement_control: self.replacement_control,
            facts: self.facts.bind(owned)?,
            child: self.child,
            child_counter: self.child_counter,
            child_condition: self.child_condition,
            child_step: self.child_step,
        })
    }
}
/// Runs actual ownership and returns only the complete original nested observation.
/// No public constructor or retrofit API exists; errors never return a sidecar.
pub fn analyze_nested_loop(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
) -> Result<Option<BoundNestedLoop>, OwnershipCheckingError> {
    let (owned, _, _, mut nested, _, _) = analysis::analyze_with_catalog(
        sources,
        inputs,
        names.names(),
        environment,
        typed.types(),
        false,
    )?;
    if inputs.len() != 1 || nested.len() != 1 {
        return Ok(None);
    }
    Ok(nested.pop().and_then(|pending| pending.bind(owned)))
}
