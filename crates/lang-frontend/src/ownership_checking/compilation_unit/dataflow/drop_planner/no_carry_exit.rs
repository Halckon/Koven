//! Ordinary planner evidence: real break cleanup, body cleanup, and target loop exit.
use super::{DropPlanner, ValueState, model::OwnedValue};
use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    ownership_checking::{
        BorrowBindingEndFact, UnitDropPoint,
        loop_no_carry::{BreakPlan, Proof},
    },
    type_checking::UnitStatementId,
};
#[derive(Default, Debug, PartialEq, Eq)]
pub(in super::super) struct Receipts {
    pub(in super::super) ends: Vec<BorrowBindingEndFact<UnitSymbolId, UnitDropPoint>>,
    pub(in super::super) exits: Vec<BreakPlan>,
}
#[derive(Default)]
pub(super) struct Tracker {
    active: Option<Active>,
    pub(super) completed: Vec<BreakPlan>,
}
struct Active {
    plan: BreakPlan,
    conditional: bool,
    entry: Vec<OwnedValue>,
    boundary: usize,
    entered: bool,
    transferred: bool,
    body_cleaned: bool,
    valid: bool,
}
// Observation-only slots are ignored; all actual ownership/loan slots remain explicit.
fn resources(state: &ValueState) -> Option<&[OwnedValue]> {
    let ValueState {
        forward_scope: _,
        multi_continuation: _,
        multi_merge: _,
        single_continuation: _,
        bounded_forward: _,
        nested_forward: _,
        branch_forward_plan: _,
        replace_consumer,
        lambda,
        borrow_bindings,
        pending_borrow_results,
        borrow_initializer_sources,
        pending_loans,
        borrow_call_transports,
        borrow_binding_transports,
        borrow_forward_transports,
        iterations,
        pending_temporaries,
        values,
        replacements,
        pending_borrows,
        pending_capture_borrows,
        closures,
        closure_asap,
        this,
    } = state;
    (replace_consumer.is_none()
        && lambda.is_none()
        && this.is_none()
        && borrow_bindings.is_empty()
        && pending_borrow_results.is_empty()
        && borrow_initializer_sources.is_empty()
        && pending_loans.is_empty()
        && borrow_call_transports.is_empty()
        && borrow_binding_transports.is_empty()
        && borrow_forward_transports.is_empty()
        && iterations.is_empty()
        && pending_temporaries.is_empty()
        && replacements.is_empty()
        && pending_borrows.is_empty()
        && pending_capture_borrows.is_empty()
        && closures.is_empty()
        && closure_asap.is_empty())
    .then_some(values)
}
impl DropPlanner<'_, '_> {
    pub(super) fn no_carry_exit_begin(
        &mut self,
        id: StatementId,
        body: StatementId,
        state: &ValueState,
    ) {
        let Some((plan, conditional)) =
            self.checker
                .no_carry_proofs
                .iter()
                .find_map(|proof| match proof {
                    Proof::BreakBranch(_, _, p)
                        if p.loop_statement == id && p.loop_body == body =>
                    {
                        Some((p.clone(), false))
                    }
                    Proof::CounterBreakBranch(_, _, _, p)
                        if p.exit.loop_statement == id && p.exit.loop_body == body =>
                    {
                        Some((p.exit.clone(), true))
                    }
                    _ => None,
                })
        else {
            return;
        };
        let Some(values) = resources(state) else {
            return;
        };
        self.no_carry_exit.active = Some(Active {
            plan,
            conditional,
            entry: values.to_vec(),
            boundary: self.scope_depth,
            entered: false,
            transferred: false,
            body_cleaned: false,
            valid: self.loop_boundaries.is_empty()
                && self
                    .owner_controls
                    .is_current(UnitStatementId::new(self.checker.source_unit, id)),
        });
    }
    pub(super) fn no_carry_transfer_before(
        &mut self,
        id: ExpressionId,
        is_continue: bool,
        state: &ValueState,
    ) {
        if let Some(active) = self.no_carry_exit.active.as_mut() {
            active.valid &= !is_continue
                && id == active.plan.break_expression
                && !active.entered
                && self.loop_boundaries.as_slice() == [active.boundary]
                && self.scope_depth == active.boundary + if active.conditional { 2 } else { 1 }
                && self.owner_controls.is_current(UnitStatementId::new(
                    self.checker.source_unit,
                    active.plan.loop_statement,
                ))
                && resources(state) == Some(active.entry.as_slice());
            active.entered = active.valid;
        }
    }
    pub(super) fn no_carry_transfer_after(
        &mut self,
        id: ExpressionId,
        is_continue: bool,
        state: &ValueState,
    ) {
        if let Some(active) = self.no_carry_exit.active.as_mut() {
            active.valid &= !is_continue
                && id == active.plan.break_expression
                && active.entered
                && !active.transferred
                && resources(state) == Some(active.entry.as_slice());
            active.transferred = active.valid;
        }
    }
    pub(super) fn no_carry_body_cleaned(
        &mut self,
        id: StatementId,
        body: StatementId,
        state: &ValueState,
        stopped: bool,
    ) {
        if let Some(active) = self.no_carry_exit.active.as_mut() {
            active.valid &= id == active.plan.loop_statement
                && body == active.plan.loop_body
                && stopped != active.conditional
                && active.transferred
                && !active.body_cleaned
                && self.scope_depth == active.boundary
                && self.loop_boundaries.as_slice() == [active.boundary]
                && resources(state) == Some(active.entry.as_slice());
            active.body_cleaned = active.valid;
        }
    }
    pub(super) fn no_carry_exit_cleaned(&mut self, id: StatementId, state: &ValueState) {
        let Some(active) = self.no_carry_exit.active.take() else {
            return;
        };
        if active.valid
            && active.body_cleaned
            && id == active.plan.loop_statement
            && self.loop_boundaries.is_empty()
            && self
                .owner_controls
                .is_current(UnitStatementId::new(self.checker.source_unit, id))
            && resources(state) == Some(active.entry.as_slice())
        {
            self.no_carry_exit.completed.push(active.plan);
        }
    }
}

impl Tracker {
    pub(super) fn finish(
        self,
        ends: Vec<BorrowBindingEndFact<UnitSymbolId, UnitDropPoint>>,
    ) -> Receipts {
        Receipts {
            ends,
            exits: self.completed,
        }
    }
}
