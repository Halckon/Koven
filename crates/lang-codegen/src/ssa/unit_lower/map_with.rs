//! source-qualified scoped callback 的实际 loan 生命周期。
use super::*;
impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_map_with_value(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let start = self.pending_operands.len();
        self.pending_call_frames
            .push(call_lifetimes::PendingCallFrame {
                call: id,
                receiver: false,
                loan_arguments: Vec::new(),
                loop_depth: self.loops.len(),
                pending_start: start,
                created_loans: Vec::new(),
                abi_slots: Vec::new(),
                field_replace_owner: None,
                shared_field_roots: Vec::new(),
                exclusive_root_owners: Vec::new(),
            });
        let result = self.lower_map_with_operands(id, span);
        let frame = self
            .pending_call_frames
            .pop()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let result = result?;
        if result != LoweredValue::Diverged {
            for index in frame.created_loans.into_iter().rev() {
                let loan = self.runtime_pending_loan(index, span)?;
                self.function
                    .append_instruction(
                        self.block,
                        Operation::BorrowEnd { loan },
                        Vec::new(),
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            }
            let owners = frame
                .abi_slots
                .iter()
                .map(|(owner, _)| require_value(self.pending_operands[*owner], span))
                .collect::<Result<Vec<_>, _>>()?;
            self.drop_abi_call_owners(owners, span)?;
            self.pending_operands.truncate(start);
            self.emit_borrow_argument_expression_drops(id)?;
            self.emit_drops(UnitDropPoint::CallReturn(id))?;
        } else {
            self.pending_operands.truncate(start);
        }
        Ok(result)
    }
    fn lower_map_with_operands(
        &mut self,
        id: UnitExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let d = *self
            .typed
            .map_with_value(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let mut slots = Vec::new();
        for operand in [d.receiver(), d.key(), d.action()] {
            if operand.source_unit() != self.source_unit {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let expression = operand.expression();
            let operand_span = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
                .span();
            if self.runtime_operand_exits(expression, operand_span)? {
                return Ok(LoweredValue::Diverged);
            }
            let ty = self.expression_ssa_type(expression, operand_span)?;
            slots.push(self.runtime_borrow_operand(id, expression, ty, operand_span, span)?);
        }
        let loans = slots
            .into_iter()
            .map(|slot| self.runtime_pending_loan(slot, span))
            .collect::<Result<Vec<_>, _>>()?;
        let ty = self.expression_ssa_type(id.expression(), span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::MapWithValue {
                    source: loans[0],
                    key: EntityId::Loan(loans[1]),
                    action: loans[2],
                },
                vec![EntityType::Value(ty)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let [EntityId::Value(result)] = results.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        Ok(LoweredValue::Value(*result))
    }
}
