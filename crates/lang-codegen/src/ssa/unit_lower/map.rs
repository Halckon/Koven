//! compilation-unit Map 容器操作、赋值与空值比较向 typed SSA 的 lowering。

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    parser::{AssignmentOperator, BinaryOperator as Operator, Expression},
    source::Span,
    type_checking::{Copyability, IntrinsicTypeConstructor, UnitExpressionId, UnitTypeKind},
};

use super::{
    LoweredValue, UnitExpressionLowerer, lowering_error, resolve_concrete_type,
    scalar::is_integer_builtin, span_key,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    integer::assignment_operator,
    model::{EntityId, EntityType, Operation, Origin, SsaTypeKind, ValueId},
};

fn require_lowered_value(value: LoweredValue, span: Span) -> Result<ValueId, LoweringError> {
    match value {
        LoweredValue::Value(v) => Ok(v),
        _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
    }
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_map_expression(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        if self.typed.map_with_value(id).is_some()
            || self.typed.map_get(id).is_some()
            || self.typed.map_consume(id).is_some()
            || self.typed.map_remove(id).is_some()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if self.typed.map_construction(id).is_some() {
            return self.lower_map_construction(expression, span).map(Some);
        }
        if self.typed.map_size(id).is_some() {
            return self.lower_map_size(expression, span).map(Some);
        }
        if self.typed.map_contains(id).is_some() {
            return self.lower_map_contains(expression, span).map(Some);
        }
        if self.typed.map_put(id).is_some() {
            return self.lower_map_put(expression, span).map(Some);
        }
        Ok(None)
    }

    fn lower_map_construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let _descriptor = self
            .typed
            .map_construction(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let map_ty = self.expression_ssa_type(expression, span)?;
        let result_id = self
            .function
            .append_instruction(
                self.block,
                Operation::MapConstruct { map_type: map_ty },
                vec![EntityType::Value(map_ty)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let Some(EntityId::Value(val)) = result_id.1.first().copied() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        self.emit_drops(UnitDropPoint::CallReturn(id))?;
        Ok(LoweredValue::Value(val))
    }

    fn lower_map_size(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .map_size(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        // A nullable Map proof view is lowered only by ssa::refine (SPEC-0397).
        if self
            .typed
            .non_null_use(descriptor.receiver())
            .is_some_and(|fact| {
                matches!(
                    self.typed.types().get(fact.declared_type()),
                    Some(UnitTypeKind::Nullable(inner))
                        if matches!(
                            self.typed.types().get(*inner),
                            Some(UnitTypeKind::Intrinsic {
                                constructor: IntrinsicTypeConstructor::Map
                                    | IntrinsicTypeConstructor::MutableMap,
                                ..
                            })
                        )
                )
            })
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner = self
            .map_owner_operand(descriptor.receiver().expression(), span)?
            .0;
        let int_ty = self.expression_ssa_type(expression, span)?;
        let result_id = self
            .function
            .append_instruction(
                self.block,
                Operation::MapSize { owner },
                vec![EntityType::Value(int_ty)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let Some(EntityId::Value(val)) = result_id.1.first().copied() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        self.emit_drops(UnitDropPoint::CallReturn(id))?;
        Ok(LoweredValue::Value(val))
    }

    fn lower_map_contains(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .map_contains(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (owner, _) = self.map_owner_operand(descriptor.receiver().expression(), span)?;
        let pending_start = self.pending_operands.len();
        self.pending_operands.push(owner);
        let key_span = self
            .parsed
            .ast()
            .expressions()
            .get(descriptor.key().expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
            .span();
        let Some((key, _)) = self.lower_map_operand(descriptor.key().expression(), key_span)?
        else {
            self.pending_operands.truncate(pending_start);
            return Ok(LoweredValue::Diverged);
        };
        let owner = self.pending_operands.get(pending_start).copied();
        self.pending_operands.truncate(pending_start);
        let owner = owner.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let bool_ty = self.expression_ssa_type(expression, span)?;
        let result_id = self
            .function
            .append_instruction(
                self.block,
                Operation::MapContains { owner, key },
                vec![EntityType::Value(bool_ty)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let Some(EntityId::Value(val)) = result_id.1.first().copied() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        self.emit_drops(UnitDropPoint::CallReturn(id))?;
        Ok(LoweredValue::Value(val))
    }

    pub(super) fn lower_map_put(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .map_put(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (owner, root) = self.extract_map_owner_val(descriptor.receiver(), span)?;
        let pending_start = self.pending_operands.len();
        self.pending_operands.push(EntityId::Value(owner));
        let lowered_key = self.lower(descriptor.key().expression())?;
        if matches!(lowered_key, LoweredValue::Diverged) {
            self.pending_operands.truncate(pending_start);
            return Ok(LoweredValue::Diverged);
        }
        let key = require_lowered_value(lowered_key, span)?;
        let key_type = super::resolve_concrete_type(
            self.typed,
            descriptor.key_type(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        if self.typed.copyability(key_type) == lang_frontend::type_checking::Copyability::MoveOnly {
            // A delivered named key is also a pending argument owner on return paths.
            self.temporaries.insert(descriptor.key(), key);
        }
        self.pending_operands.push(EntityId::Value(key));
        let lowered_val = self.lower(descriptor.value().expression())?;
        if matches!(lowered_val, LoweredValue::Diverged) {
            self.pending_operands.truncate(pending_start);
            return Ok(LoweredValue::Diverged);
        }
        let val = require_lowered_value(lowered_val, span)?;
        let (val, _) = self.adapt_owned_value_to_expected(
            descriptor.value().expression(),
            val,
            descriptor.value_type(),
            span,
        )?;
        let owner = self.pending_operands.get(pending_start).copied();
        let key = self.pending_operands.get(pending_start + 1).copied();
        self.pending_operands.truncate(pending_start);
        let (Some(EntityId::Value(owner)), Some(EntityId::Value(key))) = (owner, key) else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let receiver_span = self
            .parsed
            .ast()
            .expressions()
            .get(descriptor.receiver().expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
            .span();
        let container_type =
            self.expression_ssa_type(descriptor.receiver().expression(), receiver_span)?;
        let result_id = self
            .function
            .append_instruction(
                self.block,
                Operation::MapPut {
                    owner,
                    key,
                    value: val,
                },
                vec![EntityType::Value(container_type)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let Some(EntityId::Value(new_owner)) = result_id.1.first().copied() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        if let Some(symbol) = root {
            self.bindings.insert(symbol, LoweredValue::Value(new_owner));
        }
        // Only MoveOnly delivery retires the caller's bindings; Copyable inputs remain usable.
        let mut delivered = Vec::new();
        for (ty, value) in [(descriptor.key_type(), key), (descriptor.value_type(), val)] {
            let ty = super::resolve_concrete_type(
                self.typed,
                ty,
                self.substitutions,
                self.static_self,
                span,
            )?;
            match self.typed.copyability(ty) {
                lang_frontend::type_checking::Copyability::MoveOnly => delivered.push(value),
                lang_frontend::type_checking::Copyability::Copyable => {}
                _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            }
        }
        self.bindings.retain(|_, binding| !matches!(binding, LoweredValue::Value(value) if delivered.contains(value)));
        self.temporaries
            .retain(|_, value| !delivered.contains(value));
        self.emit_drops(UnitDropPoint::AfterExpression(descriptor.value()))?;
        self.emit_drops(UnitDropPoint::CallReturn(id))?;
        Ok(LoweredValue::Unit)
    }

    fn extract_map_owner_val(
        &mut self,
        receiver: UnitExpressionId,
        span: Span,
    ) -> Result<(ValueId, Option<UnitSymbolId>), LoweringError> {
        if receiver.source_unit() != self.source_unit {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (owner, root) = self.map_owner_operand(receiver.expression(), span)?;
        let EntityId::Value(owner_val) = owner else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        Ok((owner_val, root))
    }

    fn map_owner_operand(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<(EntityId, Option<UnitSymbolId>), LoweringError> {
        self.lower_map_operand(expression, span)?
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }

    fn lower_map_operand(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<(EntityId, Option<UnitSymbolId>)>, LoweringError> {
        let mut node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        // Locate named Borrow operands through Groups without discarding the outer
        // identity when this is a temporary requiring its own published drop fact.
        while let Expression::Group { expression } = node.payload() {
            node = self
                .parsed
                .ast()
                .expressions()
                .get(*expression)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        }
        match node.payload() {
            Expression::Name => {
                // A real constant materialization has no local storage symbol.
                // Keep the original outer expression for its use and cleanup identity.
                if self
                    .constant_materialization_origin(
                        UnitExpressionId::new(self.source_unit, expression),
                        span,
                    )?
                    .is_none()
                {
                    let symbol = self
                        .references
                        .get(&span_key(node.span()))
                        .copied()
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    if let Some(LoweredValue::Value(v)) = self.bindings.get(&symbol).copied() {
                        return Ok(Some((EntityId::Value(v), Some(symbol))));
                    }
                    if let Some(&loan) = self.borrow_bindings.get(&symbol) {
                        return Ok(Some((EntityId::Loan(loan), Some(symbol))));
                    }
                }
                let lowered = self.lower(expression)?;
                match lowered {
                    LoweredValue::Value(value) => Ok(Some((EntityId::Value(value), None))),
                    LoweredValue::Diverged => Ok(None),
                    LoweredValue::Unit => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
                }
            }
            _ => {
                let lowered = self.lower(expression)?;
                match lowered {
                    LoweredValue::Value(value) => Ok(Some((EntityId::Value(value), None))),
                    LoweredValue::Diverged => Ok(None),
                    LoweredValue::Unit => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
                }
            }
        }
    }

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
