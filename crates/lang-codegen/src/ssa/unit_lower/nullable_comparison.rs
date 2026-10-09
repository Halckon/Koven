//! source-qualified null comparison 描述符与真实 shared storage。
use super::*;
impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_nullable_comparison(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let Some(d) = self.typed.null_comparison(id) else {
            return Ok(None);
        };
        let ty = resolve_concrete_type(
            self.typed,
            d.nullable_type(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        let target = self
            .type_ids
            .get(&ty)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.map_results.contains_key(&target) {
            return Ok(None);
        }
        let operation = if let Some(&source) = self.borrow_bindings.get(&d.symbol()) {
            Operation::NullableLoanIsNull { source }
        } else if let Some(LoweredValue::Value(owner)) = self.bindings.get(&d.symbol()).copied() {
            Operation::NullableIsNull { owner }
        } else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let boolean = self.expression_ssa_type(expression, span)?;
        let result = self.append_scalar(operation, boolean, span)?;
        let result = if d.non_null_when_true() {
            let LoweredValue::Value(operand) = result else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            self.append_scalar(Operation::BooleanNot { operand }, boolean, span)?
        } else {
            result
        };
        // The descriptor bypasses operand lowering. Consume its original ASAP facts
        // only after the presence read, including the last-use nullable owner.
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Binary { left, right, .. } = *node.payload() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        self.emit_null_condition_drops(left)?;
        self.emit_null_condition_drops(right)?;
        self.emit_drops(UnitDropPoint::AfterBinaryOperands(id))?;
        Ok(Some(result))
    }
    fn emit_null_condition_drops(&mut self, expression: ExpressionId) -> Result<(), LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        if let Expression::Group { expression } = *node.payload() {
            self.emit_null_condition_drops(expression)?;
        }
        self.emit_drops(UnitDropPoint::AfterExpression(UnitExpressionId::new(
            self.source_unit,
            expression,
        )))
    }
}
