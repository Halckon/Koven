//! Map 下标按已确定的同步查询边界释放 receiver/key，不产生返回 loan。
use super::{DropExpressionUse, DropPlanner, OwnershipCheckingError, PlannerDropPoint, ValueState};
use crate::ast::ExpressionId;

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
            .map_put(self.checker.unit_expression(target))
            .expect("selected unit Map assignment descriptor");
        let receiver = descriptor.receiver().expression();
        if !self.expression(receiver, DropExpressionUse::Place, state)? {
            return Ok(false);
        }
        let root = self.checker.place(receiver)?.map(|place| place.root());
        if let Some(root) = root {
            state.pending_borrows.push((target, root));
        }
        for operand in [
            descriptor.key().expression(),
            descriptor.value().expression(),
        ] {
            if !self.expression(operand, DropExpressionUse::Consume, state)? {
                return Ok(false);
            }
            self.register_value_argument(target, operand, state)?;
        }
        state.pending_borrows.retain(|(call, _)| *call != target);
        self.finish_pending_temporaries(target, PlannerDropPoint::CallReturn(target), state);
        if let Some(root) = root
            && !self.liveness.expression_after[assignment.index()].contains(&root)
        {
            self.drop_named_asap(PlannerDropPoint::CallReturn(target), root, state);
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
            .map_get(self.checker.unit_expression(id))
            .expect("selected unit Map query descriptor");
        let mut roots = Vec::new();
        for operand in [
            descriptor.receiver().expression(),
            descriptor.key().expression(),
        ] {
            if !self.expression(operand, DropExpressionUse::Place, state)? {
                return Ok(false);
            }
            if let Some(place) = self.checker.place(operand)? {
                if state.position(place.root()).is_some() && !roots.contains(&place.root()) {
                    roots.push(place.root());
                    state.pending_borrows.push((id, place.root()));
                }
            } else if self.is_move_only_temporary(operand) {
                let origin = self.checker.parsed.ast().expressions().get(operand)?.span();
                self.register_pending_temporary(id, operand, origin, false, state)?;
            }
        }
        state.pending_borrows.retain(|(call, _)| *call != id);
        self.finish_pending_temporaries(id, PlannerDropPoint::CallReturn(id), state);
        for root in roots {
            if !self.liveness.expression_after[id.index()].contains(&root) {
                self.drop_named_asap(PlannerDropPoint::CallReturn(id), root, state);
            }
        }
        Ok(true)
    }
}
