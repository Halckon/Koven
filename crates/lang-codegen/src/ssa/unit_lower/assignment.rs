//! compilation-unit root name assignment、owner replacement 与 null comparison lowering。

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    parser::{AssignmentOperator, BinaryOperator as Operator, Expression},
    source::Span,
    type_checking::{Copyability, UnitExpressionId},
};

use super::{
    LoweredValue, UnitExpressionLowerer, lowering_error, resolve_concrete_type,
    scalar::is_integer_builtin, span_key,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    integer::assignment_operator,
    model::{Operation, SsaTypeKind},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_assignment(
        &mut self,
        expression: ExpressionId,
        target: ExpressionId,
        operator: AssignmentOperator,
        value: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if self
            .typed
            .map_put(UnitExpressionId::new(self.source_unit, target))
            .is_some()
        {
            return self.lower_map_put(target, span);
        }
        if operator == AssignmentOperator::Assign
            && self
                .typed
                .aggregate_projection(UnitExpressionId::new(self.source_unit, target))
                .is_some()
        {
            let target_span = self
                .parsed
                .ast()
                .expressions()
                .get(target)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
                .span();
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                target_span,
            ));
        }
        if self.element_place_descriptor(target)?.is_some() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let target_node = self
            .parsed
            .ast()
            .expressions()
            .get(target)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(target_node.payload(), Expression::Name) {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                target_node.span(),
            ));
        }
        let symbol = self
            .references
            .get(&span_key(target_node.span()))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, target_node.span()))?;
        let target_type = if operator == AssignmentOperator::Assign {
            let expression_id = UnitExpressionId::new(self.source_unit, expression);
            let descriptor = self
                .typed
                .assignment(expression_id)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if descriptor.expression() != expression_id
                || descriptor.target() != UnitExpressionId::new(self.source_unit, target)
                || descriptor.value() != UnitExpressionId::new(self.source_unit, value)
                || descriptor.operator() != operator
                || self.typed.symbol_type(symbol) != Some(descriptor.target_type())
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            descriptor.target_type()
        } else {
            self.typed
                .expression_type(UnitExpressionId::new(self.source_unit, target))
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, target_node.span()))?
        };

        let (assigned, transferred) = match operator {
            AssignmentOperator::Assign => match self.lower(value)? {
                LoweredValue::Value(assigned) => {
                    let (assigned, transferred) =
                        self.adapt_owned_value_to_expected(value, assigned, target_type, span)?;
                    (LoweredValue::Value(assigned), transferred)
                }
                assigned => (assigned, false),
            },
            AssignmentOperator::AddAssign
            | AssignmentOperator::SubtractAssign
            | AssignmentOperator::MultiplyAssign
            | AssignmentOperator::DivideAssign
            | AssignmentOperator::RemainderAssign => {
                if !self
                    .expression_builtin_type(target, target_node.span())?
                    .is_some_and(is_integer_builtin)
                {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        target_node.span(),
                    ));
                }
                let left = match self.bindings.get(&symbol).copied() {
                    Some(LoweredValue::Value(value)) => value,
                    Some(LoweredValue::Unit | LoweredValue::Diverged) | None => {
                        return Err(lowering_error(
                            LoweringErrorKind::MissingFact,
                            target_node.span(),
                        ));
                    }
                };
                let right = match self.lower(value)? {
                    LoweredValue::Value(value) => value,
                    LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                    LoweredValue::Unit => {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                };
                let target_ssa_type = self.expression_ssa_type(target, target_node.span())?;
                self.require_matching_assignment_type(value, target_ssa_type, span)?;
                let operator = assignment_operator(operator)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
                (
                    self.checked(operator, left, right, target_ssa_type, span)?,
                    false,
                )
            }
        };
        if assigned == LoweredValue::Diverged {
            return Ok(assigned);
        }
        if let LoweredValue::Value(value_id) = assigned
            && !transferred
        {
            self.transfer_owned_expression(value, value_id, span)?;
        }
        if self.typed.copyability(target_type) == Copyability::MoveOnly
            && self.bindings.contains_key(&symbol)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if self.nullable_proofs.contains_key(&symbol) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.bindings.insert(symbol, assigned);
        Ok(LoweredValue::Unit)
    }

    fn require_matching_assignment_type(
        &self,
        value: ExpressionId,
        target_type: crate::ssa::model::SsaTypeId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.expression_ssa_type(value, span)? != target_type {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

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
        let (operation, reads_presence) = if let Some(&source) =
            self.borrow_bindings.get(&d.symbol())
        {
            if self.nullable_maps.contains_key(&target) {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            // Copyable results keep the existing aggregate read. A MoveOnly result
            // reads absence through the loan without copying the wrapper.
            if aggregate_presence && self.typed.copyability(ty) == Copyability::Copyable {
                return Ok(None);
            }
            (Operation::NullableLoanIsNull { source }, false)
        } else if let Some(LoweredValue::Value(owner)) = self.bindings.get(&d.symbol()).copied() {
            if aggregate_presence {
                (
                    Operation::AggregateProject {
                        aggregate: owner,
                        field: 0,
                    },
                    true,
                )
            } else {
                (Operation::NullableIsNull { owner }, false)
            }
        } else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let boolean = self.expression_ssa_type(expression, span)?;
        let result = self.append_scalar(operation, boolean, span)?;
        let result = if d.non_null_when_true() != reads_presence {
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

    // A frontend result-cleanup fact identifies the owning temporary. Never invent
    // a drop from syntax or from a nullable type alone (borrow results share types).
    fn lower_temporary_nullable_comparison(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
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
        let operand = if crate::ssa::lowering_support::is_null_literal(self.parsed, right) {
            left
        } else if crate::ssa::lowering_support::is_null_literal(self.parsed, left) {
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
            && !self.nullable_maps.contains_key(&target)
            && !matches!(
                self.ssa_types.get(target.index()),
                Some(SsaTypeKind::NullableHandle { .. })
            )
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let Some(owner) = self.lower_optional_expression_value(operand)? else {
            return Ok(Some(LoweredValue::Diverged));
        };
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
