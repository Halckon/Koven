//! A distinct composed capability; neither embedded witness is listed in an older domain.
use super::*;
/// Scalar prefix paths and initialization completed before a sealed counter/exit event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoopPrefixCounterBreakFact {
    identity: Rc<()>,
    counter_break: UnitLoopCounterBreakBranchNoCarryFact,
    prefix: UnitExpressionId,
    condition: UnitExpressionId,
    calls: [UnitExpressionId; 2],
}
impl UnitLoopPrefixCounterBreakFact {
    pub(in super::super) fn new(
        counter_break: UnitLoopCounterBreakBranchNoCarryFact,
        prefix: UnitExpressionId,
        condition: UnitExpressionId,
        calls: [UnitExpressionId; 2],
    ) -> Self {
        Self {
            identity: Rc::new(()),
            counter_break,
            prefix,
            condition,
            calls,
        }
    }
    /// Exact scalar prefix control.
    pub const fn prefix(&self) -> UnitExpressionId {
        self.prefix
    }
    /// Stable Boolean condition.
    pub const fn condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// Qualified empty-body scalar calls in branch order.
    pub const fn calls(&self) -> [UnitExpressionId; 2] {
        self.calls
    }
    /// Embedded counter/exit proof, not a grant in the old domain.
    pub const fn counter_break(&self) -> &UnitLoopCounterBreakBranchNoCarryFact {
        &self.counter_break
    }
    /// One actual completion event; structural equality does not confer authority.
    pub fn same_identity(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.identity, &other.identity)
    }
}
impl CompilationUnitOwnership {
    /// Independently sealed prefix/initialization/counter/exit compositions.
    pub fn no_carry_prefix_counter_breaks(&self) -> &[UnitLoopPrefixCounterBreakFact] {
        &self.no_carry.prefix_counter_breaks
    }
    /// Only this product's exact composition identity grants permission.
    pub fn owns_no_carry_prefix_counter_break(
        &self,
        fact: &UnitLoopPrefixCounterBreakFact,
    ) -> bool {
        self.no_carry
            .prefix_counter_breaks
            .iter()
            .any(|p| p.same_identity(fact))
    }
}
