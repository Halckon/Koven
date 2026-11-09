//! 普通直接调用的观测切片；从实际 pending loan 和同一 cleanup 序列发布。
use super::{DropPlanner, PlannerDropPoint, ValueState};
use crate::{
    ast::ExpressionId,
    ownership_checking::{
        LoanKind, UnitDirectCallLoanEndFact, UnitIterationCleanupAction as Action, UnitLoanFact,
        UnitLoanTarget,
    },
    parser::{Expression, StringPart},
    type_checking::{CallableResultSource, ExpressionCategory, UnitCallTarget},
};
use std::collections::BTreeMap;

impl DropPlanner<'_, '_> {
    fn observes_direct_call_loan(&self, loan: &UnitLoanFact) -> bool {
        self.observes_call_loan(loan, LoanKind::Shared)
    }

    // Shared also admits proven temporary owners and field parents. Exclusive
    // remains root-only; both reuse registered tokens and the existing unwind traversal.
    fn observes_call_loan(&self, loan: &UnitLoanFact, kind: LoanKind) -> bool {
        let Some(call) = self.checker.typed.call(loan.call()) else {
            return false;
        };
        if loan.kind() != kind
            || call.receiver().is_some()
            || !(matches!(call.target(), UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_))
                || kind == LoanKind::Exclusive && matches!(call.target(), UnitCallTarget::External(_)))
            || call.result_source() != CallableResultSource::Owned
            // 既有 borrow/range operand 已有来源交接；本片不能替它结束祖先。
            || self.checker.borrow_result_call(loan.argument().expression()).is_some()
        {
            return false;
        }
        // 在 canonical_borrow_place/temporary_projection_owner 之前核对真实 operand。
        // 目标表示本身无法区分投影 backing、borrow binding 或 provider 的来源。
        let mut operand = loan.argument().expression();
        loop {
            let Ok(node) = self.checker.parsed.ast().expressions().get(operand) else {
                return false;
            };
            match node.payload() {
                Expression::Group { expression } => operand = *expression,
                Expression::Name => {
                    if kind == LoanKind::Shared
                        && matches!(loan.target(), UnitLoanTarget::Temporary(owner) if *owner == loan.argument())
                        && self.checker.typed.expression_category(loan.argument())
                            == Some(ExpressionCategory::Place)
                    {
                        // A Copyable nullable conversion of the name has its own temporary loan.
                        return true;
                    }
                    let Ok(Some(place)) = self.checker.place(operand) else {
                        return false;
                    };
                    return place.is_root()
                        && self.is_observable_owned_loan_root(place.root())
                        && matches!(loan.target(), UnitLoanTarget::Place(target) if target == &place);
                }
                Expression::Member { .. } => {
                    // Reuse the pending-owner proof: typed Field/Group chain, actual Shared
                    // Temporary(parent) token, and the complete parent's MoveOnly category.
                    return kind == LoanKind::Shared
                        && self
                            .borrowed_temporary_field_owner(
                                loan.call().expression(),
                                loan.argument().expression(),
                                super::UnitCallArgumentOwnershipKind::SharedLoan,
                            )
                            .is_ok_and(|owner| {
                                owner.is_some_and(|owner| {
                                    matches!(loan.target(), UnitLoanTarget::Temporary(target)
                                if *target == self.checker.unit_expression(owner))
                                })
                            });
                }
                Expression::Call { .. } => {
                    // A checked construction forms its own owner. Other call
                    // results still need a dedicated ownership-source proof.
                    return kind == LoanKind::Shared
                        && self
                            .checker
                            .construction_descriptors
                            .contains_key(&self.checker.unit_expression(operand))
                        && self.is_move_only_temporary(loan.argument().expression())
                        && matches!(loan.target(), UnitLoanTarget::Temporary(owner) if *owner == loan.argument());
                }
                Expression::String { parts } => {
                    // plain String 自己求值成 owner；不接受 interpolation 或投影。
                    return kind == LoanKind::Shared
                        && parts.iter().all(|part| matches!(part, StringPart::Text(_)))
                        && matches!(loan.target(), UnitLoanTarget::Temporary(owner) if *owner == loan.argument());
                }
                _ => return false,
            }
        }
    }

    /// 全部 operands 已完成且 call 正常返回后，退休当前 call 的实际 pending token。
    /// 提前退出仍由既有 depth-aware unwind 发布同一种 EndCallLoan action。
    pub(super) fn finish_direct_call_loans(&mut self, call: ExpressionId, state: &mut ValueState) {
        for index in (0..state.pending_loans.len()).rev() {
            let Action::EndCallLoan(loan) = &state.pending_loans[index].0 else {
                continue;
            };
            if loan.call() != self.checker.unit_expression(call)
                || !self.observes_direct_call_loan(loan)
            {
                continue;
            }
            let (action, _) = state.pending_loans.remove(index);
            self.iteration_actions
                .push((PlannerDropPoint::CallReturn(call), action));
        }
    }

    /// 不从最终 descriptor 列表推测执行；只投影 traversal 实际发布的动作。
    pub(super) fn direct_call_loan_ends(&self) -> Vec<UnitDirectCallLoanEndFact> {
        // 本 planner 只处理一个 source 的 AST；稳定下标与 point 种类组成唯一计数 key。
        let mut preceding_drops: BTreeMap<(bool, usize), usize> = BTreeMap::new();
        let mut preceding_call_loan_ends = BTreeMap::<_, usize>::new();
        let mut ends = Vec::new();
        for (point, action) in &self.iteration_actions {
            let key = match point {
                PlannerDropPoint::CallReturn(call) => (false, call.index()),
                PlannerDropPoint::ControlTransfer(transfer) => (true, transfer.index()),
                _ => continue,
            };
            let count = preceding_drops.entry(key).or_insert(0);
            match action {
                Action::Drop(_) => *count += 1,
                Action::EndCallLoan(loan) => {
                    let loan_count = preceding_call_loan_ends.entry(key).or_default();
                    if self.observes_direct_call_loan(loan) {
                        ends.push(UnitDirectCallLoanEndFact {
                            loan: loan.clone(),
                            point: point.into_unit(self.checker.source_unit),
                            preceding_drops: *count,
                            preceding_call_loan_ends: *loan_count,
                        });
                    }
                    *loan_count += 1;
                }
                _ => {}
            }
        }
        ends
    }

    /// Only actual unwind actions qualify; never infer execution from descriptors.
    pub(super) fn exclusive_call_prefix_ends(
        &self,
    ) -> Vec<crate::ownership_checking::UnitExclusiveCallPrefixEndFact> {
        let mut preceding_drops = BTreeMap::new();
        let mut preceding_call_loan_ends = BTreeMap::<_, usize>::new();
        let mut ends = Vec::new();
        for (point, action) in &self.iteration_actions {
            let PlannerDropPoint::ControlTransfer(transfer) = point else {
                continue;
            };
            if !self
                .checker
                .parsed
                .ast()
                .expressions()
                .get(*transfer)
                .is_ok_and(|node| matches!(node.payload(), Expression::Return { .. }))
            {
                continue;
            }
            let count = preceding_drops.entry(transfer.index()).or_insert(0);
            match action {
                Action::Drop(_) => *count += 1,
                Action::EndCallLoan(loan) => {
                    // Count every actual end before filtering to the Exclusive slice.
                    let loan_count = preceding_call_loan_ends
                        .entry(transfer.index())
                        .or_default();
                    if self.observes_call_loan(loan, LoanKind::Exclusive) {
                        ends.push(crate::ownership_checking::UnitExclusiveCallPrefixEndFact {
                            loan: loan.clone(),
                            point: point.into_unit(self.checker.source_unit),
                            preceding_drops: *count,
                            preceding_call_loan_ends: *loan_count,
                        });
                    }
                    *loan_count += 1;
                }
                _ => {}
            }
        }
        ends
    }
}
