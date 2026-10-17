//! 前端 null comparison 描述符驱动的非消费 presence 读取。
use super::*;
impl ExpressionLowerer<'_> {
    pub(super) fn lower_nullable_comparison(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let Some(d) = self.typed.null_comparison(expression) else {
            return self.lower_temporary_nullable_comparison(expression, span);
        };
        let ty = self.resolve_type(d.nullable_type(), span)?;
        let target = self
            .type_ids
            .get(&ty)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let aggregate_presence = self.map_results.contains_key(&target);
        let operation = if let Some(&source) = self.borrow_bindings.get(&d.symbol()) {
            if self.nullable_maps.contains_key(&target) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            // Preserve the existing borrowed aggregate path and its Copyable gate.
            if aggregate_presence {
                return Ok(None);
            }
            Operation::NullableLoanIsNull { source }
        } else if let Some(LoweredValue::Value(owner)) = self.bindings.get(&d.symbol()).copied() {
            if aggregate_presence {
                Operation::AggregateProject {
                    aggregate: owner,
                    field: 0,
                }
            } else {
                Operation::NullableIsNull { owner }
            }
        } else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let boolean = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(operation, vec![EntityType::Value(boolean)], span)?;
        let mut result = value(results[0]);
        // Old R flags mean present; NullableIsNull on handles or N means absent.
        if d.non_null_when_true() != aggregate_presence {
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

impl ExpressionLowerer<'_> {
    // A frontend result-cleanup fact identifies the owning temporary. Never invent
    // a drop from syntax or from a nullable type alone (borrow results share types).
    fn lower_temporary_nullable_comparison(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        use lang_frontend::ownership_checking::DropTarget;
        use lang_frontend::parser::BinaryOperator as Operator;
        let expressions = self.parsed.ast().expressions();
        let node = expressions
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Binary {
            left,
            operator,
            right,
            ..
        } = *node.payload()
        else {
            return Ok(None);
        };
        if !matches!(operator, Operator::Equal | Operator::NotEqual) {
            return Ok(None);
        }
        let operand = if self.is_null_literal(right) {
            left
        } else if self.is_null_literal(left) {
            right
        } else {
            return Ok(None);
        };
        let mut leaf = operand;
        let origin = loop {
            let node = expressions
                .get(leaf)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            if let Expression::Group { expression } = *node.payload() {
                leaf = expression;
            } else {
                break node.span();
            }
        };
        let point = DropPoint::AfterBinaryOperands(expression);
        let target = DropTarget::Temporary(leaf);
        let mut facts = self
            .owned
            .drops()
            .iter()
            .filter(|fact| fact.point() == point && fact.target() == target);
        let Some(fact) = facts.next() else {
            return Ok(None);
        };
        if facts.next().is_some() || fact.value_origin() != origin {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let target = self.expression_ssa_type(leaf, origin)?;
        let aggregate_presence = self.map_results.contains_key(&target);
        if !aggregate_presence
            && !self.nullable_maps.contains_key(&target)
            && !matches!(
                self.ssa_types.get(target.index()),
                Some(crate::ssa::model::SsaTypeKind::NullableHandle { .. })
            )
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner = self.require_value(operand)?;
        let operation = if aggregate_presence {
            Operation::AggregateProject {
                aggregate: owner,
                field: 0,
            }
        } else {
            Operation::NullableIsNull { owner }
        };
        let boolean = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(operation, vec![EntityType::Value(boolean)], span)?;
        let mut result = value(results[0]);
        if (operator == Operator::NotEqual) != aggregate_presence {
            let (_, results) = self.append(
                Operation::BooleanNot { operand: result },
                vec![EntityType::Value(boolean)],
                span,
            )?;
            result = value(results[0]);
        }
        self.emit_drops(point)?;
        Ok(Some(LoweredValue::Value(result)))
    }
}
