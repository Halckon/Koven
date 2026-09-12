//! Evaluated call/interpolation operands remain owned until completion or control transfer.
use super::{
    DropPlanner, OwnershipCheckingError, PlannerDropFact, PlannerDropPoint, PlannerDropTarget,
    ValueState,
};
use crate::{
    ast::ExpressionId, name_resolution::UnitSymbolId, source::Span, type_checking::Copyability,
};

#[derive(Clone, Debug)]
pub(super) struct PendingTemporary {
    pub(super) control: ExpressionId,
    expression: ExpressionId,
    origin: Span,
    transfers_at_call: bool,
    pub(super) loop_depth: usize,
    prior_symbols: Vec<UnitSymbolId>,
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
            self.register_pending_temporary(call, argument, origin, true, state);
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
    ) {
        state.pending_temporaries.push(PendingTemporary {
            control,
            expression,
            origin,
            transfers_at_call,
            loop_depth: self.loop_boundaries.len(),
            prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
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
            if !pending.transfers_at_call {
                self.push_fact(PlannerDropFact::new(
                    point,
                    PlannerDropTarget::Temporary(pending.expression),
                    pending.origin,
                ));
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
            self.push_fact(PlannerDropFact::new(
                point,
                PlannerDropTarget::Temporary(pending.expression),
                pending.origin,
            ));
        }
    }
}
