//! Evaluated call/interpolation operands remain owned until completion or control transfer.
use super::{
    DropPlanner, OwnedThis, OwnershipCheckingError, PlannerDropFact, PlannerDropPoint,
    PlannerDropTarget, ValueState,
};
use crate::{
    ast::ExpressionId, name_resolution::UnitSymbolId, source::Span, type_checking::Copyability,
};

#[derive(Clone, Debug)]
pub(super) struct PendingTemporary {
    pub(super) control: ExpressionId,
    target: PendingOwner,
    origin: Span,
    transfers_at_call: bool,
    pub(super) loop_depth: usize,
    prior_symbols: Vec<UnitSymbolId>,
    closure: Option<ExpressionId>,
    iteration_scopes: Vec<crate::type_checking::UnitStatementId>,
}

#[derive(Clone, Copy, Debug)]
enum PendingOwner {
    Temporary(ExpressionId),
    This(OwnedThis),
}

impl DropPlanner<'_, '_> {
    pub(super) fn register_value_argument(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
        state: &mut ValueState,
    ) -> Result<(), OwnershipCheckingError> {
        let unit = self.checker.unit_expression(argument);
        if self.checker.typed.expression_type(unit).is_some_and(|ty| {
            self.checker.typed.copyability(ty) == Copyability::MoveOnly
                && !self.is_stateless_object_type(ty)
        }) {
            let origin = self
                .checker
                .parsed
                .ast()
                .expressions()
                .get(argument)?
                .span();
            self.register_pending_temporary(call, argument, origin, true, state)?;
        }
        Ok(())
    }

    pub(super) fn register_pending_temporary(
        &self,
        control: ExpressionId,
        expression: ExpressionId,
        origin: Span,
        transfers_at_call: bool,
        state: &mut ValueState,
    ) -> Result<(), OwnershipCheckingError> {
        let closure = self.closure_origin(expression, state)?;
        self.register_pending_owner(
            control,
            PendingOwner::Temporary(expression),
            origin,
            transfers_at_call,
            closure,
            state,
        );
        Ok(())
    }

    pub(super) fn register_pending_this(&self, control: ExpressionId, state: &mut ValueState) {
        if let Some(receiver) = state.this.take() {
            self.register_pending_owner(
                control,
                PendingOwner::This(receiver),
                receiver.origin,
                true,
                None,
                state,
            );
        }
    }

    fn push_pending_drop(&mut self, point: PlannerDropPoint, pending: PendingTemporary) {
        if let Some(closure) = pending.closure {
            for capture in self.checker.captures_of(closure) {
                if capture.mode() == crate::ownership_checking::ClosureCaptureMode::Shared {
                    self.iteration_actions.push((
                        point,
                        crate::ownership_checking::UnitIterationCleanupAction::EndCaptureLoan {
                            closure: self.checker.unit_expression(closure),
                            source: capture.source(),
                        },
                    ));
                }
            }
        }
        match pending.target {
            PendingOwner::Temporary(expression) => {
                let target = crate::ownership_checking::UnitDropTarget::Temporary(
                    self.checker
                        .constant_temporary_origin(expression)
                        .map_or(self.checker.unit_expression(expression), |(owner, _)| owner),
                );
                self.iteration_temporary_scopes.extend(
                    pending
                        .iteration_scopes
                        .iter()
                        .map(|statement| (*statement, target, true)),
                );
                self.push_fact(PlannerDropFact::new(
                    point,
                    PlannerDropTarget::Temporary(expression),
                    pending.origin,
                ));
            }
            PendingOwner::This(receiver) => {
                self.push_this_fact_in_iteration_scopes(point, receiver, &pending.iteration_scopes)
            }
        }
    }

    fn register_pending_owner(
        &self,
        control: ExpressionId,
        target: PendingOwner,
        origin: Span,
        transfers_at_call: bool,
        closure: Option<ExpressionId>,
        state: &mut ValueState,
    ) {
        state.pending_temporaries.push(PendingTemporary {
            control,
            target,
            origin,
            transfers_at_call,
            loop_depth: self.loop_boundaries.len(),
            prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
            closure,
            iteration_scopes: state
                .iterations
                .iter()
                .map(|frame| frame.statement)
                .collect(),
        });
    }

    pub(super) fn finish_pending_temporaries(
        &mut self,
        control: ExpressionId,
        point: PlannerDropPoint,
        state: &mut ValueState,
    ) {
        for index in (0..state.pending_temporaries.len()).rev() {
            if state.pending_temporaries[index].control != control {
                continue;
            }
            let pending = state.pending_temporaries.remove(index);
            if let PendingOwner::This(receiver) = pending.target
                && receiver.conditional_type.is_some()
            {
                // 模板保留义务，具体 MoveOnly 已交付时由后端跳过；Copyable 不析构。
                state.this = Some(receiver);
            } else if !pending.transfers_at_call {
                self.push_pending_drop(point, pending);
            }
        }
    }

    /// End abandoned call protection before unwinding newer locals and operand owners.
    pub(super) fn drop_pending_temporaries(
        &mut self,
        point: PlannerDropPoint,
        state: &mut ValueState,
        selected: impl Fn(&PendingTemporary) -> bool,
    ) {
        state.pending_borrows.retain(|(call, _)| {
            !state
                .pending_temporaries
                .iter()
                .any(|pending| pending.control == *call && selected(pending))
        });
        for index in (0..state.pending_temporaries.len()).rev() {
            if !selected(&state.pending_temporaries[index]) {
                continue;
            }
            let pending = state.pending_temporaries.remove(index);
            let newer = state
                .values
                .iter()
                .filter(|value| !pending.prior_symbols.contains(&value.symbol))
                .map(|value| value.symbol)
                .collect::<Vec<_>>();
            for symbol in newer.into_iter().rev() {
                self.drop_named(point, symbol, state);
            }
            self.push_pending_drop(point, pending);
        }
    }
}
