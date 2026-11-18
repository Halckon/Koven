//! 普通 compilation-unit 短路也在两个出口发布最后使用的清理事实。
use crate::ownership_checking::compilation_unit::constant::{
    UnitShortCircuitPlan, UnitShortCircuitRhs,
};

use super::{DropExpressionUse, DropPlanner, ExpressionId, OwnershipCheckingError, ValueState};

impl DropPlanner<'_, '_> {
    // An evaluated RHS edge, not a history of the left operand or dynamic executions.
    pub(super) fn owner_short_circuit_rhs(
        &mut self,
        plan: UnitShortCircuitPlan,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let control = plan.expression.expression();
        self.enter_root_branch_arm(control, plan.rhs_branch);
        let result = self.expression(plan.right.expression(), DropExpressionUse::Read, state);
        self.leave_root_branch_arm(control, plan.rhs_branch);
        result
    }

    pub(super) fn short_circuit(
        &mut self,
        control: ExpressionId,
        plan: UnitShortCircuitPlan,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        if !self.expression(plan.left.expression(), DropExpressionUse::Read, state)? {
            return Ok(false);
        }
        let evaluated_branch = if self.checker.constant_control {
            plan.rhs_branch()
        } else {
            0
        };
        let skipped_branch = 1 - evaluated_branch;
        let mut evaluated = state.clone();
        let mut exits = Vec::with_capacity(2);
        if plan.rhs != UnitShortCircuitRhs::Never
            && self.owner_short_circuit_rhs(plan, &mut evaluated)?
        {
            self.finish_pending_temporaries(
                control,
                super::PlannerDropPoint::BranchExit {
                    control,
                    branch: evaluated_branch,
                },
                &mut evaluated,
            );
            self.drop_branch_exit(control, evaluated_branch, &mut evaluated);
            exits.push(evaluated);
        }
        if plan.rhs != UnitShortCircuitRhs::Always {
            let mut skipped = state.clone();
            self.finish_pending_temporaries(
                control,
                super::PlannerDropPoint::BranchExit {
                    control,
                    branch: skipped_branch,
                },
                &mut skipped,
            );
            self.drop_branch_exit(control, skipped_branch, &mut skipped);
            if let Some(evaluated) = exits.last() {
                Self::merge_rc_projection_temporaries(evaluated, &mut skipped);
            }
            exits.push(skipped);
        }
        let continues = !exits.is_empty();
        *state = self.merge_resource_states(control, exits);
        Ok(continues)
    }
}
