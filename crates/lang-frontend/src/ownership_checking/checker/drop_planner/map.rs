//! Map 下标查询的 receiver/key 在完成查询或控制转移后释放。
use super::{
    DropPlanner, DropPoint, ExpressionUse, LoanEndPoint, NullableTemporary, ValueState,
    pending_call,
};
use crate::{ast::ExpressionId, ownership_checking::OwnershipCheckingError};

impl DropPlanner<'_, '_> {
    pub(super) fn map_assignment(
        &mut self,
        assignment: ExpressionId,
        target: ExpressionId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let descriptor = self
            .checker
            .typed
            .map_put(target)
            .expect("selected Map assignment descriptor");
        state.pending_calls.push(pending_call::PendingCall::new(
            target,
            self.loop_boundaries.len(),
        ));
        let receiver = descriptor.receiver();
        if !self.expression(receiver, ExpressionUse::Place, state)? {
            return Ok(false);
        }
        self.register_pending_argument(
            target,
            receiver,
            crate::type_checking::ParameterMode::Inout,
            state,
        )?;
        for operand in [descriptor.key(), descriptor.value()] {
            if !self.expression(operand, ExpressionUse::Consume, state)? {
                return Ok(false);
            }
            self.register_pending_argument(
                target,
                operand,
                crate::type_checking::ParameterMode::Value,
                state,
            )?;
        }
        let roots = self.end_pending_calls(LoanEndPoint::CallReturn(target), state, |frame| {
            frame.call == target
        });
        state
            .nullable_temporaries
            .retain(|temporary| temporary.control != target || !temporary.transfers_at_call);
        self.drop_nullable_temporaries(DropPoint::CallReturn(target), state, |temporary| {
            temporary.control == target
        });
        for root in roots {
            if !self.liveness.expression_after[assignment.index()].contains(&root) {
                self.drop_named_asap(DropPoint::CallReturn(target), root, state);
            }
        }
        Ok(true)
    }
    pub(super) fn map_subscript(
        &mut self,
        id: ExpressionId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let descriptor = self
            .checker
            .typed
            .map_get(id)
            .expect("selected Map query descriptor");
        state.pending_calls.push(pending_call::PendingCall::new(
            id,
            self.loop_boundaries.len(),
        ));
        for operand in [descriptor.receiver(), descriptor.key()] {
            if !self.expression(operand, ExpressionUse::Place, state)? {
                return Ok(false);
            }
            if let Some(place) = self.checker.place(operand)? {
                if let Some(frame) = state.pending_calls.last_mut() {
                    frame.callees.push(place.root());
                }
            } else if self.is_move_only_temporary(operand) {
                state.nullable_temporaries.push(NullableTemporary {
                    versions: std::mem::take(&mut state.result_owners),
                    closures: std::mem::take(&mut state.result_closures),
                    transfers_at_call: false,
                    control: id,
                    subject: operand,
                    origin: self.checker.parsed.ast().expressions().get(operand)?.span(),
                    loop_depth: self.loop_boundaries.len(),
                    prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
                });
            }
        }
        let roots = self.end_pending_calls(LoanEndPoint::CallReturn(id), state, |frame| {
            frame.call == id
        });
        self.drop_nullable_temporaries(DropPoint::CallReturn(id), state, |temporary| {
            temporary.control == id
        });
        for root in roots {
            if !self.liveness.expression_after[id.index()].contains(&root) {
                self.drop_named_asap(DropPoint::CallReturn(id), root, state);
            }
        }
        Ok(true)
    }
}
