//! 已求值 nullable owner 在非空边交付；null 边独立承担 RHS 与 wrapper cleanup。
use super::{
    DropPlanner, ExpressionId, ExpressionUse, NullableTemporary, OwnershipCheckingError, ValueState,
};

impl DropPlanner<'_, '_> {
    pub(super) fn elvis(
        &mut self,
        id: ExpressionId,
        left: ExpressionId,
        right: ExpressionId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        if !self.expression(left, ExpressionUse::Place, state)? {
            return Ok(false);
        }
        if let Some(owner) = self.checker.temporary_element_owner(left)? {
            // Copyable element 没有 owner；其临时 backing 容器仍须在两条边清理。
            state.nullable_temporaries.push(NullableTemporary {
                versions: Vec::new(),
                closures: Vec::new(),
                transfers_at_call: false,
                control: id,
                subject: owner,
                origin: self.checker.parsed.ast().expressions().get(owner)?.span(),
                loop_depth: self.loop_boundaries.len(),
                prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
            });
        }
        let only_null = self.checker.is_only_null(left);
        let mut branches = Vec::new();
        if !only_null {
            let mut selected = state.clone();
            self.enter_branch(id, 2, 0, &mut selected)?;
            if self.checker.is_move_only_expression(left)
                && let Some(symbol) = self.checker.expression_root_symbol(left)?
            {
                selected.take(symbol);
            }
            self.drop_branch_exit(id, 0, &mut selected);
            branches.push(selected);
            self.enter_branch(id, 2, 1, state)?;
        }
        let versions = std::mem::take(&mut state.result_owners);
        let closures = std::mem::take(&mut state.result_closures);
        if self.is_move_only_temporary(left) {
            // 即使 RHS 提前 return/break，已求值的 wrapper 也有对应清理义务。
            state.nullable_temporaries.push(NullableTemporary {
                versions,
                closures,
                transfers_at_call: false,
                control: id,
                subject: left,
                origin: self.checker.parsed.ast().expressions().get(left)?.span(),
                loop_depth: self.loop_boundaries.len(),
                prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
            });
        }
        if self.expression(right, self.checker.control_result_usage(id), state)? {
            self.drop_branch_exit(id, 1, state);
            branches.push(state.clone());
        }
        let continues = !branches.is_empty();
        *state = self.merge_value_states(branches);
        Ok(continues)
    }
}
