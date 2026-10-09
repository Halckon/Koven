//! Map 容器操作向 typed SSA 的 lowering。

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{NameResolution, SymbolId},
    ownership_checking::DropPoint,
    parser::Expression,
    source::Span,
    type_checking::{IntrinsicTypeConstructor, TypeId, TypedFile},
};

use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, error,
    nominal::NominalTypeMapper, span_key, value,
};
use crate::ssa::model::{
    EntityId, EntityType, MapContainerKind, Module, Operation, SsaTypeId, ValueId,
};

impl NominalTypeMapper {
    pub(super) fn intern_map(
        &mut self,
        module: &mut Module,
        names: &NameResolution,
        typed: &TypedFile,
        constructor: IntrinsicTypeConstructor,
        arguments: &[TypeId],
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let [key, val] = arguments else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let kind = match constructor {
            IntrinsicTypeConstructor::Map => MapContainerKind::Map,
            IntrinsicTypeConstructor::MutableMap => MapContainerKind::MutableMap,
            _ => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        let nullable_value = matches!(
            typed.types().get(*val),
            Some(lang_frontend::type_checking::TypeKind::Nullable(_))
        );
        let key = self.intern_inner(module, names, typed, *key, span)?;
        let val = if matches!(
            typed.types().get(*val),
            Some(lang_frontend::type_checking::TypeKind::Nullable(_))
        ) && typed.is_resource_type(*val) == Some(true)
        {
            super::resource_deinit::validate_map_value_type(typed, *val, span)?;
            self.intern_map_result(module, names, typed, *val, span)?
        } else {
            self.intern_inner(module, names, typed, *val, span)?
        };
        // A planned query result aggregate is not a nullable Map storage representation.
        if nullable_value
            && !matches!(
                module.type_kind(val),
                Some(crate::ssa::model::SsaTypeKind::NullableHandle { .. })
            )
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        module
            .add_map_container_type(kind, key, val)
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))
    }
}

impl ExpressionLowerer<'_> {
    pub(super) fn lower_map_expression(
        &mut self,
        expression: ExpressionId,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        if self.typed.map_with_value(expression).is_some() {
            return self.lower_map_with_value(expression).map(Some);
        }
        if self.typed.map_construction(expression).is_some() {
            return self.lower_map_construction(expression).map(Some);
        }
        if self.typed.map_size(expression).is_some() {
            return self.lower_map_size(expression).map(Some);
        }
        if self.typed.map_contains(expression).is_some() {
            return self.lower_map_contains(expression).map(Some);
        }
        if self.typed.map_get(expression).is_some() {
            return self.lower_map_get(expression).map(Some);
        }
        if self.typed.map_put(expression).is_some() {
            return self.lower_map_put(expression).map(Some);
        }
        if self.typed.map_remove(expression).is_some() {
            return self.lower_map_remove(expression).map(Some);
        }
        Ok(None)
    }

    fn lower_map_construction(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let _descriptor = self
            .typed
            .map_construction(expression)
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = self.expression_span(expression)?;
        let map_ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::MapConstruct { map_type: map_ty },
            vec![EntityType::Value(map_ty)],
            span,
        )?;
        self.emit_drops(DropPoint::CallReturn(expression))?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn lower_map_size(&mut self, expression: ExpressionId) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.typed.map_size(expression).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let span = self.expression_span(expression)?;
        let owner = self.lower_map_owner(descriptor.receiver(), span)?;
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::MapSize { owner },
            vec![EntityType::Value(ty)],
            span,
        )?;
        self.emit_drops(DropPoint::CallReturn(expression))?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn lower_map_contains(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.typed.map_contains(expression).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let span = self.expression_span(expression)?;
        let owner = self.lower_map_owner(descriptor.receiver(), span)?;
        let key = self.lower_map_owner(descriptor.key(), span)?;
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::MapContains { owner, key },
            vec![EntityType::Value(ty)],
            span,
        )?;
        self.emit_drops(DropPoint::CallReturn(expression))?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn lower_map_get(&mut self, expression: ExpressionId) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.typed.map_get(expression).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let span = self.expression_span(expression)?;
        let owner = self.lower_map_owner(descriptor.receiver(), span)?;
        let key = self.lower_map_owner(descriptor.key(), span)?;
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::MapGet { owner, key },
            vec![EntityType::Value(ty)],
            span,
        )?;
        self.emit_drops(DropPoint::CallReturn(expression))?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    pub(super) fn lower_map_put(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.typed.map_put(expression).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let span = self.expression_span(expression)?;
        let (owner, symbol) = self.map_owner_for_mutation(descriptor.receiver())?;
        let key = self.require_value(descriptor.key())?;
        let val = self.require_value(descriptor.value())?;
        let val = self.adapt_owned_value_to_expected(
            descriptor.value(),
            val,
            descriptor.value_type(),
            span,
        )?;
        let container_type = self.expression_ssa_type(descriptor.receiver(), span)?;
        let (_, results) = self.append(
            Operation::MapPut {
                owner,
                key,
                value: val,
            },
            vec![EntityType::Value(container_type)],
            span,
        )?;
        let mut delivered = Vec::new();
        for (ty, value) in [(descriptor.key_type(), key), (descriptor.value_type(), val)] {
            let ty = self.resolve_type(ty, span)?;
            match self.typed.copyability(ty) {
                Some(lang_frontend::type_checking::Copyability::MoveOnly) => delivered.push(value),
                Some(lang_frontend::type_checking::Copyability::Copyable) => {}
                _ => return Err(error(LoweringErrorKind::MissingFact, span)),
            }
        }
        self.forget_delivered_owners(&delivered);
        let new_owner = value(results[0]);
        if let Some(symbol) = symbol {
            self.bindings.insert(symbol, LoweredValue::Value(new_owner));
        }
        self.emit_drops(DropPoint::AfterExpression(descriptor.value()))?;
        self.emit_drops(DropPoint::CallReturn(expression))?;
        Ok(LoweredValue::Unit)
    }

    fn lower_map_remove(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.typed.map_remove(expression).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let span = self.expression_span(expression)?;
        let (owner, symbol) = self.map_owner_for_mutation(descriptor.receiver())?;
        let key = self.lower_map_owner(descriptor.key(), span)?;
        let container_type = self.expression_ssa_type(descriptor.receiver(), span)?;
        let result_type = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::MapRemove { owner, key },
            vec![
                EntityType::Value(container_type),
                EntityType::Value(result_type),
            ],
            span,
        )?;
        let new_owner = value(results[0]);
        if let Some(symbol) = symbol {
            self.bindings.insert(symbol, LoweredValue::Value(new_owner));
        }
        self.emit_drops(DropPoint::CallReturn(expression))?;
        Ok(LoweredValue::Value(value(results[1])))
    }

    fn lower_map_owner(
        &mut self,
        receiver: ExpressionId,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(receiver)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if let Expression::Name = node.payload() {
            let symbol = self
                .references
                .get(&span_key(node.span()))
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, node.span()))?;
            if let Some(LoweredValue::Value(v)) = self.bindings.get(&symbol).copied() {
                return Ok(EntityId::Value(v));
            }
            if let Some(loan) = self.borrow_bindings.get(&symbol).copied() {
                return Ok(EntityId::Loan(loan));
            }
        }
        let val = self.require_value(receiver)?;
        Ok(EntityId::Value(val))
    }

    fn map_owner_for_mutation(
        &mut self,
        expression: ExpressionId,
    ) -> Result<(ValueId, Option<SymbolId>), LoweringError> {
        let span = self.expression_span(expression)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        match node.payload() {
            Expression::Group { expression } => self.map_owner_for_mutation(*expression),
            Expression::Name => {
                let symbol = self
                    .references
                    .get(&span_key(span))
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                match self.bindings.get(&symbol).copied() {
                    Some(LoweredValue::Value(owner)) => Ok((owner, Some(symbol))),
                    _ => Err(error(LoweringErrorKind::MissingFact, span)),
                }
            }
            _ => {
                let owner = self.require_value(expression)?;
                Ok((owner, None))
            }
        }
    }
}
