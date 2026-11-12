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
    pub(in super::super::super) private_counter:
        Option<crate::ownership_checking::UnitOwnerCounterTailEvidenceV1>,
    pub(super) segment_continuation: Option<super::counter_segment::continuation::Continuation>,
    pub(super) segment_bridge: Option<super::counter_segment::bridge::Bridge>,
    pub(in super::super::super) segment_catalog: Option<super::counter_segment::Snapshot>,
    pub(in super::super::super) counter_phase: Option<super::control_transactions::CounterPhase>,
    pub(in super::super::super) entering: bool,
    pub(in super::super::super) control: UnitStatementId,
    pub(in super::super::super) parent: Option<UnitStatementId>,
    pub(in super::super::super) root_writes: Vec<RootWriteReceipt>,
    pub(in super::super::super) normal_equation:
        Option<super::root_incoming::root_tokens::RootBoundaryEquation>,
    pub(in super::super::super) normal_retirements: Option<super::root_incoming::RootRetirements>,
    pub(in super::super::super) normal_write_vector: Option<Vec<(UnitSymbolId, UnitExpressionId)>>,
    pub(in super::super::super) root_stages: (bool, bool),
    pub(in super::super::super) incoming_roots: Option<Vec<(UnitSymbolId, UnitOwnerId)>>,
    pub(in super::super::super) closed_product: Option<UnitOwnerLoopObservationV1>,
}

#[cfg(test)]
impl DropPlanner<'_, '_> {
    pub(super) fn record_control_visit(
        &mut self,
        control: UnitStatementId,
        parent: Option<UnitStatementId>,
        entering: bool,
        closed_product: Option<UnitOwnerLoopObservationV1>,
    ) {
        if let Some(probe) = &mut self.owner_control_probe {
            probe.push(ControlVisit {
                private_counter: self.owner_loop.private_counter_evidence(control),
                segment_bridge: self.owner_loop.segment_bridge(control),
                segment_continuation: self.owner_loop.segment_continuation(control),
                segment_catalog: self.owner_loop.segment_catalog(control),
                counter_phase: self.owner_loop.counter_phase(control),
                entering,
                control,
                parent,
                closed_product,
                incoming_roots: if entering {
                    self.owner_loop.root_entry(control).map(<[_]>::to_vec)
                } else {
                    None
                },
                root_stages: if entering {
                    (false, false)
                } else {
                    self.owner_loop.root_stages(control)
                },
                normal_equation: if entering {
                    None
                } else {
                    self.owner_loop.root_equation(control)
                },
                normal_write_vector: if entering {
                    None
                } else {
                    self.owner_loop.root_normal_writes(control)
                },
                normal_retirements: if entering {
                    None
                } else {
                    self.owner_loop.root_retirements(control)
                },
                root_writes: if entering {
                    Vec::new()
                } else {
                    self.owner_loop.root_writes(control).to_vec()
                },
            });
        }
    }
}

// The traversal wrapper owns control-stack entry/exit and closed member completion.
impl DropPlanner<'_, '_> {
    pub(in super::super::super) fn owner_while_statement(
        &mut self,
        id: StatementId,
        condition: ExpressionId,
        body: StatementId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let control = UnitStatementId::new(self.checker.source_unit, id);
        #[cfg(test)]
        let parent = self.owner_controls.current();
        let previous_transaction = self.owner_loop.selected();
        let entered = self.owner_controls.enter(control);
        if entered {
            self.owner_loop.select(Some(control));
        } else {
            self.owner_loop.clear();
        }
        #[cfg(test)]
        self.record_control_visit(control, parent, true, None);
        let result = self.owner_while_statement_scoped(id, condition, body, state);
        self.owner_loop.complete_counter_member(
            control,
            state,
            entered && matches!(result, Ok(true)),
        );
        #[cfg(test)]
        let closed_product = self.owner_loop.as_ref().and_then(|t| t.result.clone());
        if entered {
            self.owner_controls.leave(control);
        }
        self.owner_loop.select(previous_transaction);
        #[cfg(test)]
        self.record_control_visit(
            control,
            self.owner_controls.current(),
            false,
            closed_product,
        );
        result
    }
}
