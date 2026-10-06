//! Observable constructor results reuse committed container descriptors and owner views.

use super::*;
use crate::ssa::model::{PlaceAccess, ValueId};
use lang_frontend::type_checking::Copyability;

impl ExpressionLowerer<'_> {
    pub(in crate::ssa::lower_frontend) fn lower_container_size(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.typed.container_size(expression).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let span = descriptor.span();
        if self.typed.expression_type(expression) != Some(descriptor.result_type())
            || builtin_type(self.typed, descriptor.result_type()) != Some(BuiltinType::Int)
            || self.typed.expression_type(descriptor.receiver())
                != Some(descriptor.container_type())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_container_identity(
            descriptor.container(),
            descriptor.element_type(),
            descriptor.container_type(),
            span,
        )?;
        let receiver_span = self.expression_span(descriptor.receiver())?;
        let (loan, ending) =
            self.lower_borrow_argument(expression, descriptor.receiver(), receiver_span)?;
        self.pending_call_loans.insert(
            (expression.index(), descriptor.receiver().index()),
            ending.then_some(loan),
        );
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::ContainerLength {
                owner: EntityId::Loan(loan),
            },
            vec![EntityType::Value(ty)],
            span,
        )?;
        self.finish_borrowed_call(expression, span)?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    /// A plain read copies only a statically Copyable element. MoveOnly elements keep Borrow access.
    pub(in crate::ssa::lower_frontend) fn lower_container_index(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let span = self.expression_span(expression)?;
        let descriptor = self
            .typed
            .element_place(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let element = self.resolve_type(descriptor.element_type(), span)?;
        if self.typed.copyability(element) != Some(Copyability::Copyable)
            || builtin_type(self.typed, element) == Some(BuiltinType::Unit)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner = self.container_view_operand(descriptor.receiver())?;
        let index = self.require_value(descriptor.index())?;
        let element = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::ContainerElementPlace { owner, index },
            vec![EntityType::Place(element)],
            span,
        )?;
        let (_, results) = self.append(
            Operation::Read {
                source: PlaceAccess::Place(place(results[0])),
            },
            vec![EntityType::Value(element)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn container_view_operand(
        &mut self,
        expression: ExpressionId,
    ) -> Result<EntityId, LoweringError> {
        let span = self.expression_span(expression)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        match node.payload() {
            Expression::Group { expression } => self.container_view_operand(*expression),
            Expression::Name => {
                let symbol = self
                    .references
                    .get(&span_key(span))
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if let Some(LoweredValue::Value(owner)) = self.bindings.get(symbol).copied() {
                    return Ok(EntityId::Value(owner));
                }
                if let Some(&loan) = self.borrow_bindings.get(symbol) {
                    return Ok(EntityId::Loan(loan));
                }
                self.require_value(expression).map(EntityId::Value)
            }
            _ => self.require_value(expression).map(EntityId::Value),
        }
    }

    pub(in crate::ssa::lower_frontend) fn lower_container_append(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .container_append(expression)
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = descriptor.span();
        let (owner, symbol) = self.container_owner_for_append(descriptor.receiver())?;
        let element = self.require_value(descriptor.element())?;
        let container_type = self.expression_ssa_type(descriptor.receiver(), span)?;
        let (_, results) = self.append(
            Operation::ContainerAppend { owner, element },
            vec![EntityType::Value(container_type)],
            span,
        )?;
        let new_owner = value(results[0]);
        if let Some(symbol) = symbol {
            self.bindings.insert(symbol, LoweredValue::Value(new_owner));
        }
        self.emit_drops(
            lang_frontend::ownership_checking::DropPoint::AfterExpression(descriptor.element()),
        )?;
        self.emit_drops(lang_frontend::ownership_checking::DropPoint::CallReturn(
            expression,
        ))?;
        Ok(LoweredValue::Unit)
    }

    pub(in crate::ssa::lower_frontend) fn lower_container_clear(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .container_clear(expression)
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = descriptor.span();
        let (owner, symbol) = self.container_owner_for_append(descriptor.receiver())?;
        let container_type = self.expression_ssa_type(descriptor.receiver(), span)?;
        let (_, results) = self.append(
            Operation::ContainerClear { owner },
            vec![EntityType::Value(container_type)],
            span,
        )?;
        let new_owner = value(results[0]);
        if let Some(symbol) = symbol {
            self.bindings.insert(symbol, LoweredValue::Value(new_owner));
        }
        self.emit_drops(lang_frontend::ownership_checking::DropPoint::CallReturn(
            expression,
        ))?;
        Ok(LoweredValue::Unit)
    }

    pub(in crate::ssa::lower_frontend) fn lower_container_remove_at(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .container_remove_at(expression)
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = descriptor.span();
        let (owner, symbol) = self.container_owner_for_append(descriptor.receiver())?;
        let index = self.require_value(descriptor.index())?;
        let element_type = self.expression_ssa_type(expression, span)?;
        let container_type = self.expression_ssa_type(descriptor.receiver(), span)?;
        let (_, results) = self.append(
            Operation::ContainerRemoveAt { owner, index },
            vec![
                EntityType::Value(element_type),
                EntityType::Value(container_type),
            ],
            span,
        )?;
        let removed_value = value(results[0]);
        let new_owner = value(results[1]);
        if let Some(symbol) = symbol {
            self.bindings.insert(symbol, LoweredValue::Value(new_owner));
        }
        self.emit_drops(
            lang_frontend::ownership_checking::DropPoint::AfterExpression(descriptor.index()),
        )?;
        self.emit_drops(lang_frontend::ownership_checking::DropPoint::CallReturn(
            expression,
        ))?;
        Ok(LoweredValue::Value(removed_value))
    }

    pub(in crate::ssa::lower_frontend) fn lower_container_remove_last(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .container_remove_last(expression)
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = descriptor.span();
        let (owner, symbol) = self.container_owner_for_append(descriptor.receiver())?;
        let element_type = self.expression_ssa_type(expression, span)?;
        let container_type = self.expression_ssa_type(descriptor.receiver(), span)?;
        let (_, results) = self.append(
            Operation::ContainerRemoveLast { owner },
            vec![
                EntityType::Value(element_type),
                EntityType::Value(container_type),
            ],
            span,
        )?;
        let removed_value = value(results[0]);
        let new_owner = value(results[1]);
        if let Some(symbol) = symbol {
            self.bindings.insert(symbol, LoweredValue::Value(new_owner));
        }
        self.emit_drops(lang_frontend::ownership_checking::DropPoint::CallReturn(
            expression,
        ))?;
        Ok(LoweredValue::Value(removed_value))
    }

    pub(in crate::ssa::lower_frontend) fn lower_container_remove_first(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .container_remove_first(expression)
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = descriptor.span();
        let (owner, symbol) = self.container_owner_for_append(descriptor.receiver())?;
        let element_type = self.expression_ssa_type(expression, span)?;
        let container_type = self.expression_ssa_type(descriptor.receiver(), span)?;
        let (_, results) = self.append(
            Operation::ContainerRemoveFirst { owner },
            vec![
                EntityType::Value(element_type),
                EntityType::Value(container_type),
            ],
            span,
        )?;
        let removed_value = value(results[0]);
        let new_owner = value(results[1]);
        if let Some(symbol) = symbol {
            self.bindings.insert(symbol, LoweredValue::Value(new_owner));
        }
        self.emit_drops(lang_frontend::ownership_checking::DropPoint::CallReturn(
            expression,
        ))?;
        Ok(LoweredValue::Value(removed_value))
    }

    fn container_owner_for_append(
        &mut self,
        expression: ExpressionId,
    ) -> Result<(ValueId, Option<lang_frontend::name_resolution::SymbolId>), LoweringError> {
        let span = self.expression_span(expression)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        match node.payload() {
            Expression::Group { expression } => self.container_owner_for_append(*expression),
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

    pub(in crate::ssa::lower_frontend) fn lower_container_expression(
        &mut self,
        expression: ExpressionId,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        if self.typed.container_size(expression).is_some() {
            return self.lower_container_size(expression).map(Some);
        }
        if self.typed.element_place(expression).is_some() {
            return self.lower_container_index(expression).map(Some);
        }
        if self.typed.container_append(expression).is_some() {
            return self.lower_container_append(expression).map(Some);
        }
        if self.typed.container_clear(expression).is_some() {
            return self.lower_container_clear(expression).map(Some);
        }
        if self.typed.container_remove_at(expression).is_some() {
            return self.lower_container_remove_at(expression).map(Some);
        }
        if self.typed.container_remove_first(expression).is_some() {
            return self.lower_container_remove_first(expression).map(Some);
        }
        if self.typed.container_remove_last(expression).is_some() {
            return self.lower_container_remove_last(expression).map(Some);
        }
        if self.typed.container_construction(expression).is_some() {
            return self.lower_container_construction(expression).map(Some);
        }
        Ok(None)
    }
}
