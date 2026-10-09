//! 短路出口分别发布清理事实，避免只在 RHS 上析构最后一次使用的 owner。
use super::{DropPlanner, ExpressionId, ExpressionUse, OwnershipCheckingError, ValueState};

impl DropPlanner<'_, '_> {
    pub(super) fn short_circuit(
        &mut self,
        control: ExpressionId,
        left: ExpressionId,
        right: ExpressionId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        if !self.expression(left, ExpressionUse::Read, state)? {
            return Ok(false);
        }
        let mut evaluated = state.clone();
        self.enter_branch(control, 2, 0, &mut evaluated)?;
        let mut exits = Vec::with_capacity(2);
        if self.expression(right, ExpressionUse::Read, &mut evaluated)? {
            self.drop_branch_exit(control, 0, &mut evaluated);
            exits.push(evaluated);
        }
        let mut skipped = state.clone();
        self.enter_branch(control, 2, 1, &mut skipped)?;
        self.drop_branch_exit(control, 1, &mut skipped);
        exits.push(skipped);
        *state = self.merge_value_states(exits);
        Ok(true)
    }
}
