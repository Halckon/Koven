//! 确定 Map 查询消费实际 receiver LoanFact，交付真实 slot loan。
use super::*;
use lang_frontend::ownership_checking::BorrowSourceLoan;
impl ExpressionLowerer<'_> {
    pub(super) fn lower_map_require_result(
        &mut self,
        expression: ExpressionId,
        source: BorrowSourceLoan<ExpressionId>,
        span: Span,
    ) -> Result<(LoanId, Vec<LoanId>), LoweringError> {
        let span = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
            .span();
        let descriptor = *self
            .typed
            .map_require_value(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if source.call() != expression || source.argument() != descriptor.receiver() {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let receiver_span = self.expression_span(descriptor.receiver())?;
        let (source_loan, created) =
            self.lower_borrow_argument(expression, descriptor.receiver(), receiver_span)?;
        let key_span = self.expression_span(descriptor.key())?;
        let (key_loan, key_created) =
            self.lower_borrow_argument(expression, descriptor.key(), key_span)?;
        let target = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::MapRequireValue {
                source: source_loan,
                key: EntityId::Loan(key_loan),
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }],
            span,
        )?;
        let [EntityId::Loan(result)] = results.as_slice() else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        if key_created {
            self.append(Operation::BorrowEnd { loan: key_loan }, Vec::new(), span)?;
        }
        self.emit_drops(DropPoint::CallReturn(expression))?;
        Ok((
            *result,
            if created {
                vec![source_loan]
            } else {
                Vec::new()
            },
        ))
    }
}
