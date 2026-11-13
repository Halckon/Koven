//! Completed single parent statement, not a loop transport or dynamic owner token.
use super::*;
use crate::ownership_checking::compilation_unit::UnitSymbolId;
use crate::ownership_checking::{UnitDropFact, UnitDropTarget};
/// Actual completion facts for one immediately resumed parent Replace declaration.
#[derive(Clone, Debug)]
pub struct UnitOwnerNestedResumeWriteV1 {
    pub(in crate::ownership_checking::compilation_unit) statement: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) call: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) root: UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit) producer: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) binding: UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit) drop: UnitDropFact,
}
impl UnitOwnerNestedResumeWriteV1 {
    /// Actual resumed declaration.
    pub fn statement(&self) -> UnitStatementId {
        self.statement
    }
    /// Actually committed Replace call.
    pub fn call(&self) -> UnitExpressionId {
        self.call
    }
    /// Live child-exit root replaced by this statement.
    pub fn root(&self) -> UnitSymbolId {
        self.root
    }
    /// Direct String expression whose completion was observed.
    pub fn producer(&self) -> UnitExpressionId {
        self.producer
    }
    /// Binding that received the returned old value.
    pub fn binding(&self) -> UnitSymbolId {
        self.binding
    }
    /// Fresh cleanup fact for the returned value.
    pub fn returned_value_drop(&self) -> UnitDropFact {
        self.drop
    }
    pub(super) fn is_consistent(&self, parent: UnitStatementId, live: &[UnitSymbolId]) -> bool {
        live.contains(&self.root)
            && !live.contains(&self.binding)
            && self.call.source_unit() == parent.source_unit()
            && self.producer.source_unit() == parent.source_unit()
            && self.statement.source_unit() == parent.source_unit()
            && self.binding.source_unit() == parent.source_unit()
            && self.drop.target() == UnitDropTarget::Named(self.binding)
            && self.drop.point() == UnitDropPoint::AfterStatement(self.statement)
    }
}
