//! 普通 compilation-unit 短路也在两个出口发布最后使用的清理事实。
use crate::ownership_checking::compilation_unit::constant::{
    UnitShortCircuitPlan, UnitShortCircuitRhs,
};

use super::{DropPlanner, ExpressionId, OwnershipCheckingError, PlannerDropPoint, ValueState};

pub(super) struct PendingRhs {
    plan: UnitShortCircuitPlan,
    cleanup: RhsCleanup,
}

enum RhsCleanup {
    Direct,
    EarlyExit(Box<ValueState>),
    Normal {
        original: Box<ValueState>,
        evaluated_branch: usize,
    },
}

impl DropPlanner<'_, '_> {
    pub(super) fn start_short_circuit_rhs(
        &mut self,
        plan: UnitShortCircuitPlan,
        state: &mut ValueState,
    ) -> Option<PendingRhs> {
        let control = plan.expression.expression();
        let early = (self.checker.constant_control && plan.rhs != UnitShortCircuitRhs::Conditional)
            || self
                .checker
                .typed
                .expression_falls_through(self.checker.unit_expression(control))
                == Some(false);
        let cleanup = if early {
            match plan.rhs {
                UnitShortCircuitRhs::Never => return None,
                UnitShortCircuitRhs::Always => RhsCleanup::Direct,
                UnitShortCircuitRhs::Conditional => {
                    let mut skipped = state.clone();
                    self.drop_branch_exit(control, 1 - plan.rhs_branch, &mut skipped);
                    RhsCleanup::EarlyExit(Box::new(skipped))
                }
            }
        } else {
            let evaluated_branch = if self.checker.constant_control {
                plan.rhs_branch()
            } else {
                0
            };
            let copy = state.clone();
            if plan.rhs == UnitShortCircuitRhs::Never {
                let mut skipped = copy;
                self.finish_short_circuit_branch(control, 1 - evaluated_branch, &mut skipped);
                *state = self.merge_resource_states(control, vec![skipped]);
                return None;
            }
            RhsCleanup::Normal {
                original: Box::new(std::mem::replace(state, copy)),
                evaluated_branch,
            }
        };
        // The active arm is the evaluated RHS edge.
        self.enter_root_branch_arm(control, plan.rhs_branch);
        Some(PendingRhs { plan, cleanup })
    }

    pub(super) fn finish_short_circuit_rhs(
        &mut self,
        pending: PendingRhs,
        result: Result<bool, OwnershipCheckingError>,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let PendingRhs { plan, cleanup } = pending;
        let control = plan.expression.expression();
        self.leave_root_branch_arm(control, plan.rhs_branch);
        match cleanup {
            RhsCleanup::Direct => result,
            RhsCleanup::EarlyExit(skipped) => {
                let mut branches = vec![*skipped];
                if result? {
                    self.drop_branch_exit(control, plan.rhs_branch, state);
                    branches.push(state.clone());
                }
                *state = self.merge_resource_states(control, branches);
                Ok(true)
            }
            RhsCleanup::Normal {
                original,
                evaluated_branch,
            } => {
                let mut evaluated = std::mem::replace(state, *original);
                let mut exits = Vec::with_capacity(2);
                if result? {
                    self.finish_short_circuit_branch(control, evaluated_branch, &mut evaluated);
                    exits.push(evaluated);
                }
                if plan.rhs != UnitShortCircuitRhs::Always {
                    let mut skipped = state.clone();
                    self.finish_short_circuit_branch(control, 1 - evaluated_branch, &mut skipped);
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
    }

    fn finish_short_circuit_branch(
        &mut self,
        control: ExpressionId,
        branch: usize,
        state: &mut ValueState,
    ) {
        self.finish_pending_temporaries(
            control,
            PlannerDropPoint::BranchExit { control, branch },
            state,
        );
        self.drop_branch_exit(control, branch, state);
    }
}
