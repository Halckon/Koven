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
            return self.lower_temporary_nullable_comparison(expression, span);
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
        let aggregate_presence = self.map_results.contains_key(&target);
        let operation = if let Some(&source) = self.borrow_bindings.get(&d.symbol()) {
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
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let boolean = self.expression_ssa_type(expression, span)?;
        let result = self.append_scalar(operation, boolean, span)?;
        // Aggregate flags mean present; pointer predicates mean absent.
        let result = if d.non_null_when_true() != aggregate_presence {
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
        if let Expression::Group { expression } = *node.payload() {
            self.emit_null_condition_drops(expression)?;
        }
        self.emit_drops(UnitDropPoint::AfterExpression(UnitExpressionId::new(
            self.source_unit,
            expression,
        )))
    }
}

impl UnitExpressionLowerer<'_> {
    // A frontend result-cleanup fact identifies the owning temporary. Never invent
    // a drop from syntax or from a nullable type alone (borrow results share types).
    fn lower_temporary_nullable_comparison(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        use lang_frontend::ownership_checking::UnitDropTarget;
        use lang_frontend::parser::BinaryOperator as Operator;
        let expressions = self.parsed.ast().expressions();
        let node = expressions
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
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
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if let Expression::Group { expression } = *node.payload() {
                leaf = expression;
            } else {
                break node.span();
            }
        };
        let point =
            UnitDropPoint::AfterBinaryOperands(UnitExpressionId::new(self.source_unit, expression));
        let target = UnitDropTarget::Temporary(UnitExpressionId::new(self.source_unit, leaf));
        let mut facts = self
            .owned
            .drops()
            .iter()
            .filter(|fact| fact.point() == point && fact.target() == target);
        let Some(fact) = facts.next() else {
            return Ok(None);
        };
        if facts.next().is_some() || fact.value_origin() != origin {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let target = self.expression_ssa_type(leaf, origin)?;
        let aggregate_presence = self.map_results.contains_key(&target);
        if !aggregate_presence
            && !matches!(
                self.ssa_types.get(target.index()),
                Some(crate::ssa::model::SsaTypeKind::NullableHandle { .. })
            )
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner = self.require_expression_value(operand)?;
        let operation = if aggregate_presence {
            Operation::AggregateProject {
                aggregate: owner,
                field: 0,
            }
        } else {
            Operation::NullableIsNull { owner }
        };
        let boolean = self.expression_ssa_type(expression, span)?;
        let mut result = self.append_scalar(operation, boolean, span)?;
        if (operator == Operator::NotEqual) != aggregate_presence {
            let LoweredValue::Value(operand) = result else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            result = self.append_scalar(Operation::BooleanNot { operand }, boolean, span)?;
        }
        self.emit_drops(point)?;
        Ok(Some(result))
    }
}
