//! Traversal-owned control stack, not owner coverage or a loop execution trace.
use super::*;
#[derive(Default)]
pub(in super::super::super) struct ControlStack {
    controls: Vec<UnitStatementId>,
    invalid: bool,
}
impl ControlStack {
    pub(in super::super::super) fn current(&self) -> Option<UnitStatementId> {
        if self.invalid {
            None
        } else {
            self.controls.last().copied()
        }
    }
    pub(in super::super::super) fn is_current(&self, control: UnitStatementId) -> bool {
        self.current() == Some(control)
    }
    pub(in super::super::super) fn balanced(&self) -> bool {
        !self.invalid && self.controls.is_empty()
    }
    pub(in super::super::super) fn enter(&mut self, control: UnitStatementId) -> bool {
        if self.invalid
            || self.controls.contains(&control)
            || self
                .controls
                .last()
                .is_some_and(|parent| parent.source_unit() != control.source_unit())
        {
            self.invalid = true;
            return false;
        }
        self.controls.push(control);
        true
    }
    pub(in super::super::super) fn leave(&mut self, control: UnitStatementId) {
        if self.controls.last() != Some(&control) {
            self.invalid = true;
            return;
        }
        self.controls.pop();
    }
}
#[cfg(test)]
#[derive(Debug)]
pub(in super::super::super) struct ControlVisit {
    pub(in super::super::super) entering: bool,
    pub(in super::super::super) control: UnitStatementId,
    pub(in super::super::super) parent: Option<UnitStatementId>,
    pub(in super::super::super) root_writes: Vec<RootWriteReceipt>,
    pub(in super::super::super) normal_retirements: Option<super::root_incoming::RootRetirements>,
    pub(in super::super::super) normal_write_vector: Option<Vec<(UnitSymbolId, UnitExpressionId)>>,
    pub(in super::super::super) root_stages: (bool, bool),
    pub(in super::super::super) incoming_roots: Option<Vec<(UnitSymbolId, UnitOwnerId)>>,
    pub(in super::super::super) closed_product: Option<UnitOwnerLoopObservationV1>,
}
