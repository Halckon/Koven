//! Traversal-owned control stack, not owner coverage or a loop execution trace.
use super::*;
#[derive(Default)]
pub(in super::super::super) struct ControlStack;
impl ControlStack {
    pub(in super::super::super) fn current(&self) -> Option<UnitStatementId> {
        None
    }
    pub(in super::super::super) fn enter(&mut self, _control: UnitStatementId) -> bool {
        true
    }
    pub(in super::super::super) fn leave(&mut self, _control: UnitStatementId) {}
}
#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(in super::super::super) struct ControlVisit {
    pub(in super::super::super) entering: bool,
    pub(in super::super::super) control: UnitStatementId,
    pub(in super::super::super) parent: Option<UnitStatementId>,
}
