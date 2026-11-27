//! Source-qualified shared field operands for Refine operations.

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{LoanKind, UnitLoanTarget},
    type_checking::{UnitAggregateProjectionKind, UnitExpressionId},
};

use super::RefineInputs;

impl RefineInputs<'_> {
    pub(super) fn is_field_projection(&self, expression: ExpressionId) -> bool {
        let Ok(root) =
            crate::ssa::borrow_result_support::ungroup(self.parsed, expression, self.span)
        else {
            return false;
        };
        self.typed
            .aggregate_projection(UnitExpressionId::new(self.source_unit, root))
            .is_some_and(|projection| projection.kind() == UnitAggregateProjectionKind::Field)
    }

    pub(super) fn has_shared_field_operand_loan(
        &self,
        consumer: ExpressionId,
        operand: ExpressionId,
    ) -> bool {
        matches!(
            self.shared_field_operand_target(consumer, operand),
            Some(UnitLoanTarget::Place(_))
        )
    }

    pub(super) fn shared_field_operand_target(
        &self,
        consumer: ExpressionId,
        operand: ExpressionId,
    ) -> Option<&UnitLoanTarget> {
        let id = |expression| UnitExpressionId::new(self.source_unit, expression);
        if !self.is_field_projection(operand) {
            return None;
        }
        self.owned
            .loans()
            .iter()
            .find(|loan| {
                loan.call() == id(consumer)
                    && loan.argument() == id(operand)
                    && loan.kind() == LoanKind::Shared
            })
            .map(|loan| loan.target())
    }
}
