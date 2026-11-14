//! Independent counter/conditional-exit domain; embedded witnesses are not old-domain grants.
use super::*;
/// Closed arms with independently traversed break cleanup and continuing decrement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoopCounterBreakBranchNoCarryFact {
    identity: Rc<()>,
    counter: UnitLoopCounterBranchNoCarryFact,
    control: UnitExpressionId,
    condition: UnitExpressionId,
    flag: UnitSymbolId,
    body: UnitStatementId,
    statement: UnitStatementId,
    expression: UnitExpressionId,
    target: UnitStatementId,
}
impl UnitLoopCounterBreakBranchNoCarryFact {
    pub(in super::super) fn new(
        unit: SourceUnitId,
        counter: UnitLoopCounterBranchNoCarryFact,
        plan: crate::ownership_checking::loop_no_carry::CounterBreakPlan<UnitSymbolId>,
    ) -> Self {
        Self {
            identity: Rc::new(()),
            counter,
            control: UnitExpressionId::new(unit, plan.control),
            condition: UnitExpressionId::new(unit, plan.condition),
            flag: plan.flag,
            body: UnitStatementId::new(unit, plan.body),
            statement: UnitStatementId::new(unit, plan.exit.break_statement),
            expression: UnitExpressionId::new(unit, plan.exit.break_expression),
            target: UnitStatementId::new(unit, plan.exit.loop_statement),
        }
    }
    /// Embedded counter witness, not an old-domain capability.
    pub const fn counter(&self) -> &UnitLoopCounterBranchNoCarryFact {
        &self.counter
    }
    /// Exact no-else conditional after both arms close.
    pub const fn break_control(&self) -> UnitExpressionId {
        self.control
    }
    /// Stable Boolean condition reference.
    pub const fn break_condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// Its resolved parameter identity.
    pub const fn break_flag(&self) -> UnitSymbolId {
        self.flag
    }
    /// The one-statement true body.
    pub const fn break_body(&self) -> UnitStatementId {
        self.body
    }
    /// The real break statement.
    pub const fn break_statement(&self) -> UnitStatementId {
        self.statement
    }
    /// The real transfer expression.
    pub const fn break_expression(&self) -> UnitExpressionId {
        self.expression
    }
    /// Exact target while.
    pub const fn target_loop(&self) -> UnitStatementId {
        self.target
    }
    /// Actual break cleanup point.
    pub const fn cleanup_point(&self) -> UnitDropPoint {
        UnitDropPoint::ControlTransfer(self.expression)
    }
    /// Independently traversed target loop exit cleanup.
    pub const fn exit_cleanup_point(&self) -> UnitDropPoint {
        UnitDropPoint::LoopExit(self.target)
    }
    /// One sealed production event, not structural equality.
    pub fn same_identity(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.identity, &other.identity)
    }
}
impl CompilationUnitOwnership {
    /// Counter loops with one post-arm conditional break, distinct from all old domains.
    pub fn no_carry_counter_break_branches(&self) -> &[UnitLoopCounterBreakBranchNoCarryFact] {
        &self.no_carry.counter_break_branches
    }
    /// Foreign equal-looking evidence cannot authorize lowering.
    pub fn owns_no_carry_counter_break_branch(
        &self,
        fact: &UnitLoopCounterBreakBranchNoCarryFact,
    ) -> bool {
        self.no_carry
            .counter_break_branches
            .iter()
            .any(|p| p.same_identity(fact))
    }
}
