//! Sealed core-checker no-carry facts. Not a runtime iteration trace or an SSA receipt.
use super::{CompilationUnitOwnership, UnitDropPoint};
use crate::{
    name_resolution::{SourceUnitId, UnitSymbolId},
    type_checking::{UnitExpressionId, UnitItemId, UnitStatementId},
};
use std::rc::Rc;

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
        &self.no_carry_loops
    }
    /// Identity ownership; equal-looking facts from another check are not interchangeable.
    pub fn owns_no_carry_loop(&self, fact: &UnitLoopNoCarryFact) -> bool {
        self.no_carry_loops
            .iter()
            .any(|owned| Rc::ptr_eq(&owned.identity, &fact.identity))
    }
}
