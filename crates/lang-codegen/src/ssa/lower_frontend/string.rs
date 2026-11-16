//! String literal, view, binary and type lowering.
use super::*;

impl ExpressionLowerer<'_> {
    pub(in crate::ssa) fn lower_string_literal(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let bytes = string_literal::decode_plain(self.parsed, self.source_text, expression)?
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let string = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::StringLiteral { string, bytes },
            vec![EntityType::Value(string)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }
    pub(in crate::ssa) fn lower_string_binary(
        &mut self,
        left: ExpressionId,
        operator: AstBinaryOperator,
        right: ExpressionId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let left = self.lower_string_view(left)?;
        let right = self.lower_string_view(right)?;
        let ty = self.expression_ssa_type(expression, span)?;
        let operation = match operator {
            AstBinaryOperator::Add => Operation::StringConcat { left, right },
            AstBinaryOperator::Equal | AstBinaryOperator::NotEqual => {
                Operation::StringEqual { left, right }
            }
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        let (_, results) = self.append(operation, vec![EntityType::Value(ty)], span)?;
        self.emit_drops(DropPoint::AfterBinaryOperands(expression))?;
        let result = value(results[0]);
        if operator != AstBinaryOperator::NotEqual {
            return Ok(LoweredValue::Value(result));
        }
        let (_, results) = self.append(
            Operation::BooleanNot { operand: result },
            vec![EntityType::Value(ty)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }
    pub(in crate::ssa) fn lower_string_view(
        &mut self,
        expression: ExpressionId,
    ) -> Result<EntityId, LoweringError> {
        // A constant name is a fresh temporary, not a local binding or borrowed parameter.
        if self
            .typed
            .constants()
            .and_then(|facts| facts.use_at(expression))
            .is_some()
        {
            return self.require_value(expression).map(EntityId::Value);
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = node.span();
        match node.payload().clone() {
            Expression::Group { expression } => self.lower_string_view(expression),
            Expression::Name => {
                let symbol = self
                    .references
                    .get(&span_key(span))
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if let Some(LoweredValue::Value(value)) = self.bindings.get(symbol).copied() {
                    return Ok(EntityId::Value(value));
                }
                self.borrow_bindings
                    .get(symbol)
                    .copied()
                    .map(EntityId::Loan)
                    .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))
            }
            _ => self.require_value(expression).map(EntityId::Value),
        }
    }
    pub(in crate::ssa) fn is_string_expression(&self, expression: ExpressionId) -> bool {
        self.typed
            .expression_type(expression)
            .and_then(|ty| self.typed.types().get(ty))
            == Some(&TypeKind::Builtin(BuiltinType::String))
    }
}
