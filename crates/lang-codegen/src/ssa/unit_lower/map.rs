//! compilation-unit Map 容器操作向 typed SSA 的 lowering。

use lang_frontend::{
    ast::ExpressionId, name_resolution::UnitSymbolId, ownership_checking::UnitDropPoint,
    parser::Expression, source::Span, type_checking::UnitExpressionId,
};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error, span_key};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, EntityType, Operation, Origin, ValueId},
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
        if self.typed.map_with_value(id).is_some() {
            return self.lower_map_with_value(expression, span).map(Some);
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
        if self.typed.map_get(id).is_some() {
            return self.lower_map_get(expression, span).map(Some);
        }
        if self.typed.map_put(id).is_some() {
            return self.lower_map_put(expression, span).map(Some);
        }
        if self.typed.map_remove(id).is_some() {
            return self.lower_map_remove(expression, span).map(Some);
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
        let (owner, _) = self.map_owner_operand(descriptor.receiver().expression(), span)?;
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
        let (key, _) = self.map_owner_operand(descriptor.key().expression(), key_span)?;
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

    fn lower_map_get(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .map_get(id)
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
        let (key, _) = self.map_owner_operand(descriptor.key().expression(), key_span)?;
        let owner = self.pending_operands.get(pending_start).copied();
        self.pending_operands.truncate(pending_start);
        let owner = owner.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let val_ty = self.expression_ssa_type(expression, span)?;
        let result_id = self
            .function
            .append_instruction(
                self.block,
                Operation::MapGet { owner, key },
                vec![EntityType::Value(val_ty)],
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
        let key = require_lowered_value(lowered_key, span)?;
        self.pending_operands.push(EntityId::Value(key));
        let lowered_val = self.lower(descriptor.value().expression())?;
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

    fn lower_map_remove(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .map_remove(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (owner, root) = self.extract_map_owner_val(descriptor.receiver(), span)?;
        let pending_start = self.pending_operands.len();
        self.pending_operands.push(EntityId::Value(owner));
        let key_span = self
            .parsed
            .ast()
            .expressions()
            .get(descriptor.key().expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
            .span();
        let (key, _) = self.map_owner_operand(descriptor.key().expression(), key_span)?;
        let owner = self.pending_operands.get(pending_start).copied();
        self.pending_operands.truncate(pending_start);
        let Some(EntityId::Value(owner)) = owner else {
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
        let result_type = self.expression_ssa_type(expression, span)?;
        let result_id = self
            .function
            .append_instruction(
                self.block,
                Operation::MapRemove { owner, key },
                vec![
                    EntityType::Value(container_type),
                    EntityType::Value(result_type),
                ],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let Some(EntityId::Value(new_owner)) = result_id.1.first().copied() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        if let Some(symbol) = root {
            self.bindings.insert(symbol, LoweredValue::Value(new_owner));
        }
        self.emit_drops(UnitDropPoint::CallReturn(id))?;
        let Some(EntityId::Value(result)) = result_id.1.get(1).copied() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        Ok(LoweredValue::Value(result))
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
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        match node.payload() {
            Expression::Group { expression } => self.map_owner_operand(*expression, span),
            Expression::Name => {
                let symbol = self
                    .references
                    .get(&span_key(node.span()))
                    .copied()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                if let Some(LoweredValue::Value(v)) = self.bindings.get(&symbol).copied() {
                    return Ok((EntityId::Value(v), Some(symbol)));
                }
                if let Some(&loan) = self.borrow_bindings.get(&symbol) {
                    return Ok((EntityId::Loan(loan), Some(symbol)));
                }
                let lowered = self.lower(expression)?;
                let val = require_lowered_value(lowered, span)?;
                Ok((EntityId::Value(val), None))
            }
            _ => {
                let lowered = self.lower(expression)?;
                let val = require_lowered_value(lowered, span)?;
                Ok((EntityId::Value(val), None))
            }
        }
    }
}
