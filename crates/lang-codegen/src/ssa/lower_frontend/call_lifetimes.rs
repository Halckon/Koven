//! 按 frontend 的精确控制转移事实结束调用前缀借用。
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{DropPoint, LoanEndPoint},
};

use super::{ExpressionLowerer, LoweringError, LoweringErrorKind, Operation, error};

impl ExpressionLowerer<'_> {
    /// 结束当前路径的调用前缀借用；兄弟分支由入口快照恢复。
    pub(super) fn emit_control_transfer_cleanup(
        &mut self,
        transfer: ExpressionId,
    ) -> Result<(), LoweringError> {
        let span = self.expression_span(transfer)?;
        let endings = self
            .owned
            .loan_ends()
            .iter()
            .filter(|fact| fact.point() == LoanEndPoint::ControlTransfer(transfer))
            .map(|fact| (fact.call().index(), fact.argument().index()))
            .collect::<Vec<_>>();
        for key in endings {
            let loan = self
                .pending_call_loans
                .remove(&key)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if let Some(loan) = loan {
                self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
            }
        }
        // A transfer leaves proofs introduced inside its target scope. Owners may
        // survive a break/continue and must not keep that branch-local view alive.
        let is_return = matches!(
            self.parsed
                .ast()
                .expressions()
                .get(transfer)
                .map(|node| node.payload()),
            Ok(lang_frontend::parser::Expression::Return { .. })
        );
        let views = self
            .non_null_bindings
            .iter()
            .filter_map(|(&symbol, &loan)| {
                let retained = !is_return
                    && self
                        .loops
                        .last()
                        .is_some_and(|context| context.entry_views.contains_key(&symbol));
                (!retained).then_some((symbol, loan))
            })
            .collect::<Vec<_>>();
        for (symbol, loan) in views {
            self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
            self.non_null_bindings.remove(&symbol);
        }
        self.emit_drops(DropPoint::ControlTransfer(transfer))
    }
}
