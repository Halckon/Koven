//! 确定 Map 查询消费实际 receiver LoanFact，交付真实 slot loan。
use super::*;
use lang_frontend::ownership_checking::BorrowSourceLoan;
impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_map_require_result(
        &mut self,
        expression: ExpressionId,
        source: BorrowSourceLoan<UnitExpressionId>,
        span: Span,
    ) -> Result<(LoanId, Vec<LoanId>), LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let span = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
            .span();
        let descriptor = *self
            .typed
            .map_require_value(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if source.call() != id
            || source.argument() != descriptor.receiver()
            || descriptor.receiver().source_unit() != self.source_unit
            || descriptor.key().source_unit() != self.source_unit
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let receiver = descriptor.receiver().expression();
        let receiver_span = self
            .parsed
            .ast()
            .expressions()
            .get(receiver)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
            .span();
        let receiver_type = self.expression_ssa_type(receiver, receiver_span)?;
        let (source_loan, created, _) =
            self.lower_borrow_argument(id, receiver, receiver_type, receiver_span, span)?;
        let key = descriptor.key().expression();
        let key_span = self
            .parsed
            .ast()
            .expressions()
            .get(key)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
            .span();
        let key_type = self.expression_ssa_type(key, key_span)?;
        let (key_loan, key_created, _) =
            self.lower_borrow_argument(id, key, key_type, key_span, span)?;
        let target = self.expression_ssa_type(expression, span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::MapRequireValue {
                    source: source_loan,
                    key: EntityId::Loan(key_loan),
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let [EntityId::Loan(result)] = results.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        for loan in key_created.into_iter().rev() {
            self.function
                .append_instruction(
                    self.block,
                    Operation::BorrowEnd { loan },
                    Vec::new(),
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        }
        self.emit_drops(UnitDropPoint::CallReturn(id))?;
        Ok((*result, created))
    }
}
