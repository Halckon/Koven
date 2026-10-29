//! 普通直接调用的观测切片；从实际 pending loan 和同一 cleanup 序列发布。
use super::{DropPlanner, PlannerDropPoint, ValueState};
use crate::{
    ast::ExpressionId,
    name_resolution::SymbolKind,
    ownership_checking::{
        LoanKind, OwnershipBindingKind, UnitDirectCallLoanEndFact,
        UnitIterationCleanupAction as Action, UnitLoanFact, UnitLoanTarget,
    },
    parser::{Expression, StringPart},
    type_checking::{CallableResultSource, UnitCallTarget},
};
use std::collections::BTreeMap;

impl DropPlanner<'_, '_> {
    fn observes_direct_call_loan(&self, loan: &UnitLoanFact) -> bool {
        let Some(call) = self.checker.typed.call(loan.call()) else {
            return false;
        };
        if loan.kind() != LoanKind::Shared
            || call.receiver().is_some()
            || !matches!(call.target(), UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_))
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
                    let Ok(Some(place)) = self.checker.place(operand) else {
                        return false;
                    };
                    let root = place.root();
                    let owned =
                        self.checker.formed_owned_locals.contains(&root)
                            || self.checker.bindings.get(&root).is_some_and(|binding| {
                                binding.kind() == OwnershipBindingKind::Owned
                            });
                    let provider = self.checker.iterations.values().any(|plan| {
                        plan.bindings().iter().any(|binding| binding.symbol() == root)
                            || matches!(plan.source(), Some(UnitLoanTarget::Place(source)) if source.root() == root)
                    }) || self.checker.map_iterations.values().any(|plan| {
                        plan.bindings().iter().any(|binding| binding.symbol() == root)
                            || matches!(plan.source(), Some(UnitLoanTarget::Place(source)) if source.root() == root)
                    });
                    return owned
                        && !provider
                        && matches!(
                            self.checker.symbol_kind(root),
                            Some(SymbolKind::Variable | SymbolKind::ValueParameter)
                        )
                        && place.is_root()
                        && !self
                            .checker
                            .borrow_results
                            .bindings
                            .iter()
                            .any(|binding| binding.binding() == root)
                        && matches!(loan.target(), UnitLoanTarget::Place(target) if target == &place);
                }
                Expression::String { parts } => {
                    // plain String 自己求值成 owner；不接受 interpolation、投影或构造调用。
                    return parts.iter().all(|part| matches!(part, StringPart::Text(_)))
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
        let mut preceding_drops = BTreeMap::new();
        let mut ends = Vec::new();
        for (point, action) in &self.iteration_actions {
            let key = match point {
                PlannerDropPoint::CallReturn(call) => (false, *call),
                PlannerDropPoint::ControlTransfer(transfer) => (true, *transfer),
                _ => continue,
            };
            let count = preceding_drops.entry(key).or_insert(0);
            match action {
                Action::Drop(_) => *count += 1,
                Action::EndCallLoan(loan) if self.observes_direct_call_loan(loan) => {
                    ends.push(UnitDirectCallLoanEndFact {
                        loan: loan.clone(),
                        point: point.into_unit(self.checker.source_unit),
                        preceding_drops: *count,
                    });
                }
                _ => {}
            }
        }
        ends
    }
}
