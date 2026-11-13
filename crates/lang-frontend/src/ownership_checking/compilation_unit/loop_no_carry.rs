//! Sealed core-checker no-carry facts. Not a runtime iteration trace or an SSA receipt.
mod counter_break;
mod prefix_counter_break;
use super::{CompilationUnitOwnership, UnitDropPoint};
use crate::{
    name_resolution::{SourceUnitId, UnitSymbolId},
    type_checking::{UnitExpressionId, UnitItemId, UnitStatementId},
};
pub use counter_break::UnitLoopCounterBreakBranchNoCarryFact;
pub use prefix_counter_break::UnitLoopPrefixCounterBreakFact;
use std::rc::Rc;

/// One publication gate for both independent no-carry proof domains.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct CompletedNoCarry {
    pub(super) prefix_counter_breaks: Vec<UnitLoopPrefixCounterBreakFact>,
    pub(super) loops: Vec<UnitLoopNoCarryFact>,
    pub(super) branches: Vec<UnitLoopBranchNoCarryFact>,
    pub(super) counter_branches: Vec<UnitLoopCounterBranchNoCarryFact>,
    pub(super) break_prefixes: Vec<UnitLoopBreakPrefixFact>,
    pub(super) counter_break_branches: Vec<UnitLoopCounterBreakBranchNoCarryFact>,
    pub(super) break_branches: Vec<UnitLoopBreakBranchNoCarryFact>,
}
impl CompletedNoCarry {
    pub(super) fn extend(&mut self, other: Self) {
        self.prefix_counter_breaks
            .extend(other.prefix_counter_breaks);
        self.loops.extend(other.loops);
        self.branches.extend(other.branches);
        self.counter_branches.extend(other.counter_branches);
        self.break_branches.extend(other.break_branches);
        self.counter_break_branches
            .extend(other.counter_break_branches);
        self.break_prefixes.extend(other.break_prefixes);
    }
}

/// A real body frame closed its one local borrow before cleanup and restored its normal edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoopNoCarryFact {
    identity: Rc<()>,
    callable: UnitItemId,
    loop_statement: UnitStatementId,
    body: UnitStatementId,
    binding: UnitSymbolId,
    source: UnitSymbolId,
    initializer: UnitExpressionId,
    source_argument: UnitExpressionId,
    end: UnitDropPoint,
    consumers: Vec<(UnitStatementId, UnitExpressionId, UnitExpressionId)>,
}
impl UnitLoopNoCarryFact {
    pub(super) fn new(
        unit: SourceUnitId,
        plan: crate::ownership_checking::loop_no_carry::Plan<UnitSymbolId>,
    ) -> Option<Self> {
        let last = plan.consumers.last()?.0;
        Some(Self {
            identity: Rc::new(()),
            callable: UnitItemId::new(unit, plan.callable),
            loop_statement: UnitStatementId::new(unit, plan.loop_statement),
            body: UnitStatementId::new(unit, plan.body),
            binding: plan.binding,
            source: plan.root,
            initializer: UnitExpressionId::new(unit, plan.source_call),
            source_argument: UnitExpressionId::new(unit, plan.source_argument),
            end: UnitDropPoint::AfterStatement(UnitStatementId::new(unit, last)),
            consumers: plan
                .consumers
                .into_iter()
                .map(|(s, c, a)| {
                    (
                        UnitStatementId::new(unit, s),
                        UnitExpressionId::new(unit, c),
                        UnitExpressionId::new(unit, a),
                    )
                })
                .collect(),
        })
    }
    /// Whether both facts originate from the same core-checker validation event.
    pub fn same_identity(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.identity, &other.identity)
    }
    /// Source-qualified declaration containing the checked loop.
    pub const fn callable(&self) -> UnitItemId {
        self.callable
    }
    /// Exact while statement checked by the core protocol.
    pub const fn loop_statement(&self) -> UnitStatementId {
        self.loop_statement
    }
    /// Actual scope claimed before checking its elements.
    pub const fn body(&self) -> UnitStatementId {
        self.body
    }
    /// The sole local result binding, not an instance created at the header.
    pub const fn binding(&self) -> UnitSymbolId {
        self.binding
    }
    /// Stable external Borrow String parameter.
    pub const fn source(&self) -> UnitSymbolId {
        self.source
    }
    /// Actual source-loan call.
    pub const fn initializer(&self) -> UnitExpressionId {
        self.initializer
    }
    /// Actual source-loan operand.
    pub const fn source_argument(&self) -> UnitExpressionId {
        self.source_argument
    }
    /// Actual last-use End, independently matched against the final planner fact.
    pub const fn end(&self) -> UnitDropPoint {
        self.end
    }
    /// Ordered actual normal consumer returns: statement, call and operand identities.
    pub fn consumers(&self) -> &[(UnitStatementId, UnitExpressionId, UnitExpressionId)] {
        &self.consumers
    }
}
impl CompilationUnitOwnership {
    /// Core-validated finite straight-line loop-body no-carry facts, independent of observation.
    pub fn no_carry_loops(&self) -> &[UnitLoopNoCarryFact] {
        &self.no_carry.loops
    }
    /// Identity ownership; equal-looking facts from another check are not interchangeable.
    pub fn owns_no_carry_loop(&self, fact: &UnitLoopNoCarryFact) -> bool {
        self.no_carry
            .loops
            .iter()
            .any(|owned| Rc::ptr_eq(&owned.identity, &fact.identity))
    }
}

/// Two distinct arm frames, each closed before joining the same empty-loan backedge.
/// Not an SSA receipt, a borrow phi or a carried-loan capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoopBranchNoCarryFact {
    identity: Rc<()>,
    branch: UnitExpressionId,
    arms: [UnitLoopNoCarryFact; 2],
}
impl UnitLoopBranchNoCarryFact {
    pub(super) fn new(branch: UnitExpressionId, arms: [UnitLoopNoCarryFact; 2]) -> Option<Self> {
        (arms[0].loop_statement == arms[1].loop_statement
            && arms[0].callable == arms[1].callable
            && arms[0].source == arms[1].source
            && arms[0].body != arms[1].body
            && arms[0].binding != arms[1].binding
            && arms[0].end != arms[1].end
            && !arms[0].same_identity(&arms[1]))
        .then(|| Self {
            identity: Rc::new(()),
            branch,
            arms,
        })
    }
    /// Exact selected if/else, not an arbitrary conditional with similar branches.
    pub const fn branch(&self) -> UnitExpressionId {
        self.branch
    }
    /// Ordered then/else identities and actual End facts.
    pub fn arms(&self) -> &[UnitLoopNoCarryFact; 2] {
        &self.arms
    }
    /// Both values must originate in this same validation event.
    pub fn same_identity(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.identity, &other.identity)
    }
}
impl CompilationUnitOwnership {
    /// Independently closed if/else arm pairs, separate from straight-body facts.
    pub fn no_carry_branches(&self) -> &[UnitLoopBranchNoCarryFact] {
        &self.no_carry.branches
    }
    /// Foreign equivalent-looking facts cannot confer this check's authority.
    pub fn owns_no_carry_branch(&self, fact: &UnitLoopBranchNoCarryFact) -> bool {
        self.no_carry
            .branches
            .iter()
            .any(|owned| owned.same_identity(fact))
    }
}

/// Independently closed arms with one exact bounded scalar counter update.
/// Not a carried-loan proof or an SSA/native receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoopCounterBranchNoCarryFact {
    identity: Rc<()>,
    pair: UnitLoopBranchNoCarryFact,
    counter: UnitSymbolId,
    rounds: UnitSymbolId,
    initializer: UnitExpressionId,
    initial_statement: UnitStatementId,
    condition: UnitExpressionId,
    update: UnitStatementId,
    decrement: UnitExpressionId,
    decrement_operand: UnitExpressionId,
}
impl UnitLoopCounterBranchNoCarryFact {
    pub(super) fn new(
        unit: SourceUnitId,
        pair: UnitLoopBranchNoCarryFact,
        plan: crate::ownership_checking::loop_no_carry::CounterPlan<UnitSymbolId>,
    ) -> Self {
        Self {
            identity: Rc::new(()),
            pair,
            counter: plan.counter,
            rounds: plan.rounds,
            initializer: UnitExpressionId::new(unit, plan.initializer),
            initial_statement: UnitStatementId::new(unit, plan.initial_statement),
            condition: UnitExpressionId::new(unit, plan.condition),
            update: UnitStatementId::new(unit, plan.update),
            decrement: UnitExpressionId::new(unit, plan.decrement),
            decrement_operand: UnitExpressionId::new(unit, plan.decrement_operand),
        }
    }
    /// The separately closed arm pair, not owned by the legacy branch domain.
    pub const fn pair(&self) -> &UnitLoopBranchNoCarryFact {
        &self.pair
    }
    /// Exact local mutable counter symbol.
    pub const fn counter(&self) -> UnitSymbolId {
        self.counter
    }
    /// Stable Int input parameter.
    pub const fn rounds(&self) -> UnitSymbolId {
        self.rounds
    }
    /// Exact rounds reference initializing the counter.
    pub const fn initializer(&self) -> UnitExpressionId {
        self.initializer
    }
    /// Adjacent local counter declaration statement.
    pub const fn initial_statement(&self) -> UnitStatementId {
        self.initial_statement
    }
    /// Exact builtin greater-than-zero loop condition.
    pub const fn condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// Actual sole decrement assignment statement, after both arms close.
    pub const fn update(&self) -> UnitStatementId {
        self.update
    }
    /// Exact subtract-one RHS.
    pub const fn decrement(&self) -> UnitExpressionId {
        self.decrement
    }
    /// Counter reference read by the decrement.
    pub const fn decrement_operand(&self) -> UnitExpressionId {
        self.decrement_operand
    }
    /// Identity from the same validation event, not structural equivalence.
    pub fn same_identity(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.identity, &other.identity)
    }
}
impl CompilationUnitOwnership {
    /// Separate bounded-counter proof domain; legacy Boolean-loop APIs remain unchanged.
    pub fn no_carry_counter_branches(&self) -> &[UnitLoopCounterBranchNoCarryFact] {
        &self.no_carry.counter_branches
    }
    /// Equal-looking facts from a foreign check cannot grant authority.
    pub fn owns_no_carry_counter_branch(&self, fact: &UnitLoopCounterBranchNoCarryFact) -> bool {
        self.no_carry
            .counter_branches
            .iter()
            .any(|owned| owned.same_identity(fact))
    }
}

/// The only selected non-repeating completion in the no-carry break domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitLoopNoCarryExitKind {
    /// Two closed arms followed by a real unconditional break.
    Break,
}
/// Closed arms plus independently traversed transfer, scope cleanup and loop exit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoopBreakBranchNoCarryFact {
    identity: Rc<()>,
    pair: UnitLoopBranchNoCarryFact,
    loop_body: UnitStatementId,
    break_statement: UnitStatementId,
    break_expression: UnitExpressionId,
    exit_target: UnitStatementId,
}
impl UnitLoopBreakBranchNoCarryFact {
    pub(super) fn new(
        unit: SourceUnitId,
        pair: UnitLoopBranchNoCarryFact,
        plan: crate::ownership_checking::loop_no_carry::BreakPlan,
    ) -> Option<Self> {
        let target = UnitStatementId::new(unit, plan.loop_statement);
        (pair.arms()[0].loop_statement() == target && pair.arms()[1].loop_statement() == target)
            .then(|| Self {
                identity: Rc::new(()),
                pair,
                loop_body: UnitStatementId::new(unit, plan.loop_body),
                break_statement: UnitStatementId::new(unit, plan.break_statement),
                break_expression: UnitExpressionId::new(unit, plan.break_expression),
                exit_target: target,
            })
    }
    /// Pair remains outside all legacy no-carry proof domains.
    pub const fn pair(&self) -> &UnitLoopBranchNoCarryFact {
        &self.pair
    }
    /// Exact while body whose scope cleanup completed.
    pub const fn loop_body(&self) -> UnitStatementId {
        self.loop_body
    }
    /// Actual tail break statement.
    pub const fn break_statement(&self) -> UnitStatementId {
        self.break_statement
    }
    /// Actual control-transfer expression, not a synthetic edge.
    pub const fn break_expression(&self) -> UnitExpressionId {
        self.break_expression
    }
    /// The traversed target while identity, not a next-statement guess.
    pub const fn exit_target(&self) -> UnitStatementId {
        self.exit_target
    }
    /// Alias naming the actual target while directly.
    pub const fn target_loop(&self) -> UnitStatementId {
        self.exit_target
    }
    /// Cleanup at the actual transfer, before its target loop exit.
    pub const fn cleanup_point(&self) -> UnitDropPoint {
        UnitDropPoint::ControlTransfer(self.break_expression)
    }
    /// Cleanup at the actual target loop exit.
    pub const fn exit_cleanup_point(&self) -> UnitDropPoint {
        UnitDropPoint::LoopExit(self.exit_target)
    }
    /// This domain never grants a normal backedge or Continue capability.
    pub const fn exit_kind(&self) -> UnitLoopNoCarryExitKind {
        UnitLoopNoCarryExitKind::Break
    }
    /// Identity from one validation event; equality alone is not authority.
    pub fn same_identity(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.identity, &other.identity)
    }
}
impl CompilationUnitOwnership {
    /// Separately sealed terminal-break pairs, never listed in the other three domains.
    pub fn no_carry_break_branches(&self) -> &[UnitLoopBreakBranchNoCarryFact] {
        &self.no_carry.break_branches
    }
    /// Foreign structurally equal facts confer no authority.
    pub fn owns_no_carry_break_branch(&self, fact: &UnitLoopBreakBranchNoCarryFact) -> bool {
        self.no_carry
            .break_branches
            .iter()
            .any(|owned| owned.same_identity(fact))
    }
}

/// Independent scalar-prefix completion bound to one sealed tail-break event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoopBreakPrefixFact {
    identity: Rc<()>,
    pair: UnitLoopBreakBranchNoCarryFact,
    prefix: UnitExpressionId,
    condition: UnitExpressionId,
    calls: [UnitExpressionId; 2],
}
impl UnitLoopBreakPrefixFact {
    pub(super) fn new(
        pair: UnitLoopBreakBranchNoCarryFact,
        prefix: UnitExpressionId,
        condition: UnitExpressionId,
        calls: [UnitExpressionId; 2],
    ) -> Self {
        Self {
            identity: Rc::new(()),
            pair,
            prefix,
            condition,
            calls,
        }
    }
    /// Exact prefix if validated on both real paths.
    pub const fn prefix(&self) -> UnitExpressionId {
        self.prefix
    }
    /// Already typed Boolean parameter condition.
    pub const fn condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// The two independent scalar calls, in arm order.
    pub const fn calls(&self) -> [UnitExpressionId; 2] {
        self.calls
    }
    /// The exact terminal-break proof this prefix enters.
    pub const fn pair(&self) -> &UnitLoopBreakBranchNoCarryFact {
        &self.pair
    }
    /// Identity of one validation event, not structural authority.
    pub fn same_identity(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.identity, &other.identity)
    }
}
impl CompilationUnitOwnership {
    /// Independently completed scalar prefixes of tail-break loops.
    pub fn no_carry_break_prefixes(&self) -> &[UnitLoopBreakPrefixFact] {
        &self.no_carry.break_prefixes
    }
    /// Structurally equal foreign facts confer no authority.
    pub fn owns_no_carry_break_prefix(&self, fact: &UnitLoopBreakPrefixFact) -> bool {
        self.no_carry
            .break_prefixes
            .iter()
            .any(|p| p.same_identity(fact))
            && self.owns_no_carry_break_branch(fact.pair())
    }
}
