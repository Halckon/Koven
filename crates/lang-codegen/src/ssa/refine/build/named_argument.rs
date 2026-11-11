//! Existing ordinary Borrow argument producer, with optional P3 identity verification.
use super::*;
use crate::ssa::model::LoanKind;
impl Builder<'_, '_> {
    pub(super) fn named_argument_loan(
        &mut self,
        call: UnitExpressionId,
        value_expression: ExpressionId,
        value: Slot,
        parameter_type: UnitTypeId,
        call_arg_span: Span,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let value = self.adapt_borrowed(value, value_expression, parameter_type)?;
        self.body.named_plans.enter(
            call,
            self.unit(value_expression),
            value,
            &self.bindings,
            span,
        )?;
        self.body.temporary_plans.enter(
            call,
            self.unit(value_expression),
            value,
            &self.temporaries,
            span,
        )?;
        let target = self.body.slots[value.0];
        let EntityType::Value(target) = target else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        // The loan originates at the borrowed argument; it ends at the call.
        let argument_span = self.span(value_expression);
        let place = self.value(
            ROp::RootPlace { owner: value },
            EntityType::Place(target),
            call_arg_span,
        )?;
        let loan = self.value(
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
            argument_span,
        )?;
        Ok(loan)
    }
}
