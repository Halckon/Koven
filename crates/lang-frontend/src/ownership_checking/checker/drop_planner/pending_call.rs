//! 调用前缀持有的 loan 与 temporary 随分支状态流动。
use super::{
    DropPlanner, LoanEndFact, LoanEndPoint, LoanTarget, NullableTemporary, OwnershipCheckingError,
    ValueState,
};
use crate::{
    ast::ExpressionId,
    name_resolution::SymbolId,
    ownership_checking::LoanFact,
    type_checking::{Copyability, ParameterMode},
};

#[derive(Clone, Debug)]
pub(super) struct PendingCall {
    pub(super) call: ExpressionId,
    pub(super) loop_depth: usize,
    pub(super) loans: Vec<LoanFact>,
}
impl PendingCall {
    pub(super) fn new(call: ExpressionId, loop_depth: usize) -> Self {
        Self {
            call,
            loop_depth,
            loans: Vec::new(),
        }
    }
}
impl DropPlanner<'_, '_> {
    pub(super) fn register_pending_argument(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
        mode: ParameterMode,
        state: &mut ValueState,
    ) -> Result<(), OwnershipCheckingError> {
        // Until the call actually happens, a consumed argument remains an evaluation obligation.
        if mode == ParameterMode::Value
            && self
                .checker
                .typed
                .expression_type(argument)
                .and_then(|ty| self.checker.typed.copyability(ty))
                == Some(Copyability::MoveOnly)
        {
            state.nullable_temporaries.push(NullableTemporary {
                transfers_at_call: true,
                control: call,
                subject: argument,
                origin: self
                    .checker
                    .parsed
                    .ast()
                    .expressions()
                    .get(argument)?
                    .span(),
                loop_depth: self.loop_boundaries.len(),
                prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
            });
        }
        let loans = self
            .checker
            .loans
            .iter()
            .filter(|loan| loan.call() == call && loan.argument() == argument)
            .cloned()
            .collect::<Vec<_>>();
        for loan in &loans {
            if let LoanTarget::Temporary(subject) = loan.target()
                && self.is_move_only_temporary(*subject)
            {
                state.nullable_temporaries.push(NullableTemporary {
                    transfers_at_call: false,
                    control: call,
                    subject: *subject,
                    origin: self
                        .checker
                        .parsed
                        .ast()
                        .expressions()
                        .get(*subject)?
                        .span(),
                    loop_depth: self.loop_boundaries.len(),
                    prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
                });
            }
        }
        if let Some(frame) = state
            .pending_calls
            .iter_mut()
            .rev()
            .find(|frame| frame.call == call)
        {
            frame.loans.extend(loans);
        }
        Ok(())
    }
    /// All selected loans end before any corresponding owner cleanup is published.
    pub(super) fn end_pending_calls(
        &mut self,
        point: LoanEndPoint,
        state: &mut ValueState,
        selected: impl Fn(&PendingCall) -> bool,
    ) -> Vec<SymbolId> {
        let mut roots = Vec::new();
        for index in (0..state.pending_calls.len()).rev() {
            if !selected(&state.pending_calls[index]) {
                continue;
            }
            let frame = state.pending_calls.remove(index);
            for loan in frame.loans {
                let fact = LoanEndFact {
                    call: loan.call(),
                    argument: loan.argument(),
                    point,
                };
                if !self.loan_ends.contains(&fact) {
                    self.loan_ends.push(fact);
                }
                if let LoanTarget::Place(place) = loan.target()
                    && !roots.contains(&place.root())
                {
                    roots.push(place.root());
                }
            }
        }
        roots
    }
}
