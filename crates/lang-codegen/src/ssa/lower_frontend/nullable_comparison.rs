//! 前端 null comparison 描述符驱动的非消费 presence 读取。
use super::*;
impl ExpressionLowerer<'_> {
    pub(super) fn lower_nullable_comparison(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let Some(d) = self.typed.null_comparison(expression) else {
            return Ok(None);
        };
        let ty = self.resolve_type(d.nullable_type(), span)?;
        let target = self
            .type_ids
            .get(&ty)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self.map_results.contains_key(&target) {
            return Ok(None);
        }
        let operation = if let Some(&source) = self.borrow_bindings.get(&d.symbol()) {
            Operation::NullableLoanIsNull { source }
        } else if let Some(LoweredValue::Value(owner)) = self.bindings.get(&d.symbol()).copied() {
            Operation::NullableIsNull { owner }
        } else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let boolean = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(operation, vec![EntityType::Value(boolean)], span)?;
        let mut result = value(results[0]);
        if d.non_null_when_true() {
            let (_, results) = self.append(
                Operation::BooleanNot { operand: result },
                vec![EntityType::Value(boolean)],
                span,
            )?;
            result = value(results[0]);
        }
        // The descriptor bypasses operand lowering. Consume its original ASAP facts
        // only after the presence read, including the last-use nullable owner.
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Binary { left, right, .. } = *node.payload() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        self.emit_null_condition_drops(left)?;
        self.emit_null_condition_drops(right)?;
        self.emit_drops(DropPoint::AfterBinaryOperands(expression))?;
        Ok(Some(LoweredValue::Value(result)))
    }
    // The discriminator replaces condition evaluation, but its ASAP cleanup still
    // belongs after the read, on each runtime branch, before the branch body.
    pub(super) fn emit_null_condition_drops(
        &mut self,
        expression: ExpressionId,
    ) -> Result<(), LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        match node.payload().clone() {
            Expression::Binary { left, right, .. } => {
                self.emit_null_condition_drops(left)?;
                self.emit_null_condition_drops(right)?;
            }
            Expression::Group { expression } => self.emit_null_condition_drops(expression)?,
            _ => {}
        }
        self.emit_drops(lang_frontend::ownership_checking::DropPoint::AfterExpression(expression))
    }
}
