//! Borrowed facts only. Construction remains inside the complete split-source bound.
use super::*;

/// Read-only callee projection tied to a borrowed `BoundSplitNamedEntry`.
/// No constructor, ownership conversion, or single-source publication authority is exposed.
pub struct SplitNamedCallee<'a> {
    pub(super) callee: &'a Callee,
}

impl SplitNamedCallee<'_> {
    /// Original traversal order; arrays are never normalized into the other order.
    #[must_use]
    pub const fn order(&self) -> NamedEntryOrder {
        match self.callee {
            Callee::Before(_) => NamedEntryOrder::BeforeChild,
            Callee::After(_) => NamedEntryOrder::AfterChild,
        }
    }
    /// Actual callable, parent loop and child loop, all source-qualified.
    #[must_use]
    pub const fn controls(&self) -> (UnitItemId, UnitStatementId, UnitStatementId) {
        match self.callee {
            Callee::Before(p) => (p.callable, p.controls[0], p.controls[1]),
            Callee::After(p) => (p.callable, p.controls[0], p.controls[1]),
        }
    }
    /// Replaced root, parent counter, child counter and parent parameter.
    #[must_use]
    pub const fn roots(&self) -> (UnitSymbolId, UnitSymbolId, UnitSymbolId, UnitSymbolId) {
        match self.callee {
            Callee::Before(p) => (
                p.root,
                p.counters[0],
                p.counters[1],
                p.counter_parameters[0],
            ),
            Callee::After(p) => (
                p.root,
                p.counters[0],
                p.counters[1],
                p.counter_parameters[0],
            ),
        }
    }
    /// Actual parent and child parameters, in role order.
    #[must_use]
    pub const fn counter_parameters(&self) -> [UnitSymbolId; 2] {
        match self.callee {
            Callee::Before(p) => p.counter_parameters,
            Callee::After(p) => p.counter_parameters,
        }
    }
    /// Actual parent and child counter initializer expressions.
    #[must_use]
    pub const fn counter_initializers(&self) -> [UnitExpressionId; 2] {
        match self.callee {
            Callee::Before(p) => p.counter_initializers,
            Callee::After(p) => p.counter_initializers,
        }
    }
    /// Parent then child condition/step expressions; not CFG edge authority.
    #[must_use]
    pub const fn counter_expressions(&self) -> [(UnitExpressionId, UnitExpressionId); 2] {
        match self.callee {
            Callee::Before(p) => [(p.conditions[0], p.steps[0]), (p.conditions[1], p.steps[1])],
            Callee::After(p) => [(p.conditions[0], p.steps[0]), (p.conditions[1], p.steps[1])],
        }
    }
    /// Actual Replace calls in traversal order: parent/child or child/parent.
    #[must_use]
    pub const fn replacements(&self) -> [UnitExpressionId; 2] {
        match self.callee {
            Callee::Before(p) => p.calls,
            Callee::After(p) => p.calls,
        }
    }
    /// Initial site followed by incoming sites in traversal order; not dynamic generations.
    #[must_use]
    pub const fn producer_sites(&self) -> [UnitExpressionId; 3] {
        match self.callee {
            Callee::Before(p) => p.sites,
            Callee::After(p) => p.sites,
        }
    }
    /// Detached-old cleanups in traversal order, followed by final replaced-root cleanup.
    #[must_use]
    pub const fn cleanup(&self) -> &[UnitDropFact; 3] {
        match self.callee {
            Callee::Before(p) => &p.cleanup,
            Callee::After(p) => &p.cleanup,
        }
    }
    /// Exclusive registrations in traversal order; not independent evidence of loan End.
    #[must_use]
    pub const fn loans(&self) -> &[UnitLoanFact; 2] {
        match self.callee {
            Callee::Before(p) => &p.loans,
            Callee::After(p) => &p.loans,
        }
    }
    /// Second root with its actual suffix use and cleanup.
    #[must_use]
    pub const fn retained(&self) -> &RetainedLoopRoot {
        match self.callee {
            Callee::Before(p) => &p.retained,
            Callee::After(p) => &p.retained,
        }
    }
    /// Actual final replaced-root use. The split binder requires this for both orders.
    /// The initial source site is not the current dynamic loop generation.
    #[must_use]
    pub const fn final_root_use(&self) -> Option<&RetainedLoopRoot> {
        match self.callee {
            Callee::Before(p) => p.final_root_use.as_ref(),
            Callee::After(p) => Some(&p.final_root_use),
        }
    }
}
