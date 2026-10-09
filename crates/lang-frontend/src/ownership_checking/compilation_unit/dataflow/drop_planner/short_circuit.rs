//! 普通 compilation-unit 短路也在两个出口发布最后使用的清理事实。
use super::{DropExpressionUse, DropPlanner, ExpressionId, OwnershipCheckingError, ValueState};

impl DropPlanner<'_, '_> {
    pub(super) fn short_circuit(
        &mut self,
        control: ExpressionId,
        left: ExpressionId,
        right: ExpressionId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        if !self.expression(left, DropExpressionUse::Read, state)? {
            return Ok(false);
        }
        let mut evaluated = state.clone();
        let mut exits = Vec::with_capacity(2);
        if self.expression(right, DropExpressionUse::Read, &mut evaluated)? {
            self.drop_branch_exit(control, 0, &mut evaluated);
            exits.push(evaluated);
        }
        let mut skipped = state.clone();
        self.drop_branch_exit(control, 1, &mut skipped);
        exits.push(skipped);
        *state = self.merge_resource_states(control, exits);
        Ok(true)
    }
}
