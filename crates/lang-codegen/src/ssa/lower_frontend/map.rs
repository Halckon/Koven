//! Map 容器操作向 typed SSA 的 lowering。

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{NameResolution, SymbolId},
    ownership_checking::DropPoint,
    parser::Expression,
    source::Span,
    type_checking::{
        ExpressionCategory, IntrinsicTypeConstructor, MapSizeDescriptor, TypeId, TypedFile,
    },
};

use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, error,
    nominal::NominalTypeMapper, span_key, value,
};
use crate::ssa::model::{
    EntityId, EntityType, MapContainerKind, Module, Operation, SsaTypeId, SsaTypeKind, ValueId,
};

struct PendingMapOperand {
    entity: PendingMapEntity,
    source_loan: Option<(usize, usize)>,
}

enum PendingMapEntity {
    Copyable(EntityId),
    Owner { expression: usize, retained: bool },
    Borrowed(SymbolId),
}

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
        if self.typed.map_consume(expression).is_some() {
            return self.lower_map_consume(expression).map(Some);
        }
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
        let owner = match self.nullable_map_size_operand(expression, descriptor, span)? {
            Some(owner) => owner,
            None => self.lower_map_owner(descriptor.receiver(), span)?,
        };
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::MapSize { owner },
            vec![EntityType::Value(ty)],
            span,
        )?;
        self.emit_drops(DropPoint::CallReturn(expression))?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    /// Read an existing current proof only for registered nullable Map storage.
    /// Birth and transport keep their existing proof-origin checks; this query
    /// neither creates a proof nor retires it before the original cleanup point.
    fn nullable_map_size_operand(
        &self,
        expression: ExpressionId,
        descriptor: &MapSizeDescriptor,
        span: Span,
    ) -> Result<Option<EntityId>, LoweringError> {
        let mut operand = descriptor.receiver();
        let mut operands = Vec::new();
        let root = loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            operands.push(operand);
            match node.payload() {
                Expression::Group { expression } => operand = *expression,
                Expression::Name => {
                    break self
                        .references
                        .get(&span_key(node.span()))
                        .copied()
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                }
                _ => return Ok(None),
            }
        };
        let storage = self
            .typed
            .symbol_type(root)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(self.typed.types().get(storage),
        Some(lang_frontend::type_checking::TypeKind::Nullable(inner))
            if matches!(self.typed.types().get(*inner),
                Some(lang_frontend::type_checking::TypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Map | IntrinsicTypeConstructor::MutableMap,
                    ..
                })))
        {
            return Ok(None);
        }
        let declared = self.resolve_type(storage, span)?;
        let Some(&nullable) = self.type_ids.get(&declared) else {
            return Ok(None);
        };
        let Some(&inner) = self.nullable_maps.get(&nullable) else {
            return Ok(None);
        };
        // A recognized nullable Map read must not fall back to its wrapper Value.
        let use_fact = self
            .typed
            .non_null_use(descriptor.receiver())
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let proof = self
            .non_null_proof_for_expression(descriptor.receiver(), span)?
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let narrowed = self.resolve_type(use_fact.narrowed_type(), span)?;
        if descriptor.expression() != expression
            || use_fact.symbol() != root
            || use_fact.declared_type() != storage
            || self.resolve_type(descriptor.map_type(), span)? != narrowed
            || !matches!(self.typed.types().get(declared),
                Some(lang_frontend::type_checking::TypeKind::Nullable(target)) if *target == narrowed)
            || self.type_ids.get(&narrowed) != Some(&inner)
            || proof.inner != inner
            || self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|entity| entity.ty)
                != Some(EntityType::Value(nullable))
            || !matches!(
                self.ssa_types.get(inner.index()),
                Some(SsaTypeKind::MapContainer { .. })
            )
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if self.borrow_bindings.contains_key(&root) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        for operand in operands {
            let fact = self
                .typed
                .non_null_use(operand)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if fact.expression() != operand
                || fact.symbol() != root
                || fact.declared_type() != storage
                || fact.narrowed_type() != use_fact.narrowed_type()
                || self.typed.expression_type(operand) != Some(fact.narrowed_type())
                || self.typed.expression_category(operand) != Some(ExpressionCategory::Place)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(Some(EntityId::Loan(proof.loan)))
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
        let owner = self.preserve_map_operand(expression, descriptor.receiver(), owner, span)?;
        let Some(key) = self.lower_map_operand(descriptor.key(), span)? else {
            return Ok(LoweredValue::Diverged);
        };
        let owner = self.restore_map_operand(owner, span)?;
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
        let owner = self.preserve_map_operand(expression, descriptor.receiver(), owner, span)?;
        let Some(key) = self.lower_map_operand(descriptor.key(), span)? else {
            return Ok(LoweredValue::Diverged);
        };
        let owner = self.restore_map_operand(owner, span)?;
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
        let owner = self.preserve_map_operand(
            expression,
            descriptor.receiver(),
            EntityId::Value(owner),
            span,
        )?;
        let key = match self.lower(descriptor.key())? {
            LoweredValue::Value(value) => value,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => {
                return Err(error(
                    LoweringErrorKind::UnsupportedNode,
                    self.expression_span(descriptor.key())?,
                ));
            }
        };
        let key =
            self.preserve_map_operand(expression, descriptor.key(), EntityId::Value(key), span)?;
        let val = match self.lower(descriptor.value())? {
            LoweredValue::Value(value) => value,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => {
                return Err(error(
                    LoweringErrorKind::UnsupportedNode,
                    self.expression_span(descriptor.value())?,
                ));
            }
        };
        let val = self.adapt_owned_value_to_expected(
            descriptor.value(),
            val,
            descriptor.value_type(),
            span,
        )?;
        let EntityId::Value(owner) = self.restore_map_operand(owner, span)? else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let EntityId::Value(key) = self.restore_map_operand(key, span)? else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
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
        let owner = self.preserve_map_operand(
            expression,
            descriptor.receiver(),
            EntityId::Value(owner),
            span,
        )?;
        let Some(key) = self.lower_map_operand(descriptor.key(), span)? else {
            return Ok(LoweredValue::Diverged);
        };
        let EntityId::Value(owner) = self.restore_map_operand(owner, span)? else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
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

    // Later arguments may introduce CFG edges. Reuse the existing carry slots rather
    // than retaining an owner/loan ID from the receiver's original block.
    fn preserve_map_operand(
        &mut self,
        call: ExpressionId,
        expression: ExpressionId,
        operand: EntityId,
        span: Span,
    ) -> Result<PendingMapOperand, LoweringError> {
        // Map operations use owners or existing loans directly. The frontend still
        // publishes this exact call-prefix loan identity, including reservations.
        // Register its cleanup obligation without inventing or ending an SSA loan.
        let source_loan = self
            .owned
            .loan_begin(expression)
            .filter(|fact| fact.call() == call && fact.argument() == expression)
            .map(|fact| (fact.call().index(), fact.argument().index()));
        if let Some(key) = source_loan
            && self.pending_call_loans.insert(key, None).is_some()
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let entity = match operand {
            EntityId::Value(owner) => {
                let ty = self
                    .typed
                    .expression_type(expression)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let ty = self.resolve_type(ty, span)?;
                match self.typed.copyability(ty) {
                    Some(lang_frontend::type_checking::Copyability::Copyable) => {
                        PendingMapEntity::Copyable(operand)
                    }
                    Some(lang_frontend::type_checking::Copyability::MoveOnly) => {
                        let retained = self.temporaries.insert(expression.index(), owner).is_some();
                        PendingMapEntity::Owner {
                            expression: expression.index(),
                            retained,
                        }
                    }
                    _ => return Err(error(LoweringErrorKind::MissingFact, span)),
                }
            }
            EntityId::Loan(loan) => {
                // Borrow bindings already have CFG carriers. Reusing one must never
                // make this operation responsible for ending the caller's loan.
                let symbol = self
                    .borrow_bindings
                    .iter()
                    .find_map(|(&symbol, &binding)| (binding == loan).then_some(symbol))
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                PendingMapEntity::Borrowed(symbol)
            }
            EntityId::Place(_) => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        Ok(PendingMapOperand {
            entity,
            source_loan,
        })
    }

    fn restore_map_operand(
        &mut self,
        operand: PendingMapOperand,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        if let Some(key) = operand.source_loan
            && self.pending_call_loans.remove(&key) != Some(None)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let operand = match operand.entity {
            PendingMapEntity::Copyable(operand) => Some(operand),
            PendingMapEntity::Owner {
                expression,
                retained,
            } => {
                let owner = if retained {
                    self.temporaries.get(&expression).copied()
                } else {
                    self.temporaries.remove(&expression)
                };
                owner.map(EntityId::Value)
            }
            PendingMapEntity::Borrowed(symbol) => self
                .borrow_bindings
                .get(&symbol)
                .copied()
                .map(EntityId::Loan),
        };
        operand.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    fn lower_map_owner(
        &mut self,
        receiver: ExpressionId,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        self.lower_map_operand(receiver, span)?
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))
    }

    fn lower_map_operand(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<EntityId>, LoweringError> {
        let mut node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        // Groups are transparent only when locating an existing Borrow binding.
        // A temporary must still lower its outer expression to publish its cleanup alias.
        while let Expression::Group { expression } = node.payload() {
            node = self
                .parsed
                .ast()
                .expressions()
                .get(*expression)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        }
        if let Expression::Name = node.payload() {
            let symbol = self
                .references
                .get(&span_key(node.span()))
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, node.span()))?;
            if let Some(LoweredValue::Value(v)) = self.bindings.get(&symbol).copied() {
                return Ok(Some(EntityId::Value(v)));
            }
            if let Some(loan) = self.borrow_bindings.get(&symbol).copied() {
                return Ok(Some(EntityId::Loan(loan)));
            }
        }
        match self.lower(expression)? {
            LoweredValue::Value(value) => Ok(Some(EntityId::Value(value))),
            LoweredValue::Diverged => Ok(None),
            LoweredValue::Unit => Err(error(LoweringErrorKind::UnsupportedNode, span)),
        }
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
