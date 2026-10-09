//! 同步 callback 的 receiver/key/action 实际 loans 及调用后清理。
use super::*;
impl ExpressionLowerer<'_> {
    pub(super) fn lower_map_with_value(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let span = self.expression_span(expression)?;
        let d = *self
            .typed
            .map_with_value(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let mut loans = Vec::new();
        for operand in [d.receiver(), d.key(), d.action()] {
            let operand_span = self.expression_span(operand)?;
            if self.runtime_operand_exits(operand, operand_span)? {
                return Ok(LoweredValue::Diverged);
            }
            let (loan, new) = self.lower_borrow_argument(expression, operand, operand_span)?;
            loans.push(loan);
            self.pending_call_loans
                .insert((expression.index(), operand.index()), new.then_some(loan));
        }
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::MapWithValue {
                source: loans[0],
                key: EntityId::Loan(loans[1]),
                action: loans[2],
            },
            vec![EntityType::Value(ty)],
            span,
        )?;
        self.finish_borrowed_call(expression, span)?;
        Ok(LoweredValue::Value(value(results[0])))
    }
}
