//! `Map<K, V>` and `MutableMap<K, V>` operations in the refined CFG: construction, `.size`,
//! `.contains(key)`, `.get(key)` / `m[key]`, `.put(key, value)` / `m[key] = value`,
//! `.remove(key)`, `.consume()`, `.withValue(key, action)`, and `$map.result` comparisons.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::UnitDropPoint,
    parser::{BinaryOperator, Expression},
    source::Span,
    type_checking::{BuiltinType, UnitTypeId, UnitTypeKind},
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{ComparisonOperator, EntityType, LoanKind, ScalarConstant, SsaTypeId},
    refine::body::{ROp, Slot},
};

impl Builder<'_, '_> {
    pub(super) fn map_operation(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<Lowered>, LoweringError> {
        let id = self.unit(expression);
        let typed = self.inputs.typed;
        if typed.map_construction(id).is_some() {
            return self.map_construction(expression, span).map(Some);
        }
        if typed.map_size(id).is_some() {
            return self.map_size(expression, span).map(Some);
        }
        if typed.map_contains(id).is_some() {
            return self.map_contains(expression, span).map(Some);
        }
        if typed.map_put(id).is_some() {
            return self.map_put(expression, span).map(Some);
        }
        if typed.map_consume(id).is_some() {
            return self.map_consume(expression, span).map(Some);
        }
        if typed.map_get(id).is_some() {
            return self.map_get(expression, span).map(Some);
        }
        if typed.map_remove(id).is_some() {
            return self.map_remove(expression, span).map(Some);
        }
        if typed.map_with_value(id).is_some() {
            return self.map_with_value(expression, span).map(Some);
        }
        Ok(None)
    }

    pub(super) fn is_map_put_target(&self, target: ExpressionId) -> bool {
        self.inputs.typed.map_put(self.unit(target)).is_some()
    }

    fn drop_receiver_chain(&mut self, mut receiver: ExpressionId) -> Result<(), LoweringError> {
        let mut chain = vec![receiver];
        while let Ok((Expression::Group { expression: inner }, _)) = self.node(receiver) {
            receiver = inner;
            chain.push(receiver);
        }
        for expr in chain.into_iter().rev() {
            self.drops(UnitDropPoint::AfterExpression(self.unit(expr)))?;
        }
        Ok(())
    }

    fn map_construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let map_type = self.ssa_type(self.expression_type(expression)?, span)?;
        let slot = self.value(
            ROp::MapConstruct { map_type },
            EntityType::Value(map_type),
            span,
        )?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(slot))
    }

    fn map_size(&mut self, expression: ExpressionId, span: Span) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .map_size(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let receiver = descriptor.receiver().expression();
        let view = self.narrowed_view(receiver)?;
        let (owner, drop_receiver) = if let Some(view) = view {
            (view, true)
        } else if let Ok(symbol) = self.named_container_symbol(receiver, span) {
            (
                self.borrowed
                    .get(&symbol)
                    .or_else(|| self.bindings.get(&symbol))
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                false,
            )
        } else {
            let Some(slot) = self.required(receiver)? else {
                return Ok(Lowered::Diverged);
            };
            (slot, true)
        };
        let result_ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let result = self.value(ROp::MapSize { owner }, EntityType::Value(result_ty), span)?;
        if drop_receiver {
            self.drop_receiver_chain(receiver)?;
        }
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(result))
    }

    fn map_contains(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .map_contains(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let receiver = descriptor.receiver().expression();
        let view = self.narrowed_view(receiver)?;
        let (owner, drop_receiver) = if let Some(view) = view {
            (view, true)
        } else if let Ok(symbol) = self.named_container_symbol(receiver, span) {
            (
                self.borrowed
                    .get(&symbol)
                    .or_else(|| self.bindings.get(&symbol))
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                false,
            )
        } else {
            let Some(slot) = self.required(receiver)? else {
                return Ok(Lowered::Diverged);
            };
            (slot, true)
        };
        let key_expr = descriptor.key().expression();
        let key = if let Some(loan) = self.borrowed_string_loan(key_expr)? {
            loan
        } else {
            let Some(slot) = self.required(key_expr)? else {
                return Ok(Lowered::Diverged);
            };
            slot
        };
        let result_ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let result = self.value(
            ROp::MapContains { owner, key },
            EntityType::Value(result_ty),
            span,
        )?;
        if drop_receiver {
            self.drop_receiver_chain(receiver)?;
        }
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(result))
    }

    pub(super) fn map_put(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .map_put(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let key_expr = descriptor.key().expression();
        let Some(key) = self.required(key_expr)? else {
            return Ok(Lowered::Diverged);
        };
        let value_expr = descriptor.value().expression();
        let Some(val) = self.required(value_expr)? else {
            return Ok(Lowered::Diverged);
        };
        let value_ty = self.c(descriptor.value_type());
        let val = self.adapt(val, value_expr, value_ty)?;
        if self.move_only(self.c(descriptor.key_type())) {
            self.deliver(key_expr);
        }
        if self.move_only(value_ty) {
            self.deliver(value_expr);
        }
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.push(
            ROp::MapPut {
                owner: binding,
                key,
                value: val,
            },
            vec![binding],
            span,
        )?;
        self.drops(UnitDropPoint::AfterExpression(descriptor.value()))?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Unit)
    }

    fn map_consume(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .map_consume(id)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let receiver = descriptor.receiver().expression();
        let symbol = self.named_container_symbol(receiver, span)?;
        if let Some(view) = self.narrowed_view(receiver)? {
            self.active_views.retain(|v| v.slot != view);
            for frame in &mut self.loops {
                frame.views = frame.views.min(self.active_views.len());
            }
            let wrapper = *self
                .bindings
                .get(&symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let inner_ssa = self.ssa_type(self.c(descriptor.receiver_type()), span)?;
            let owner = self.value(
                ROp::NullableTake {
                    owner: wrapper,
                    proof: view,
                },
                EntityType::Value(inner_ssa),
                self.span(receiver),
            )?;
            self.sync_dead();
            self.dead.insert(symbol);
            self.drop_receiver_chain(receiver)?;
            let map_type = self.ssa_type(self.expression_type(expression)?, span)?;
            let result = self.value(
                ROp::MapConsume { owner, map_type },
                EntityType::Value(map_type),
                span,
            )?;
            self.drops(UnitDropPoint::CallReturn(id))?;
            return Ok(Lowered::Value(result));
        }
        let Some(owner) = self.required(receiver)? else {
            return Ok(Lowered::Diverged);
        };
        self.deliver(receiver);
        self.sync_dead();
        self.dead.insert(symbol);
        let map_type = self.ssa_type(self.expression_type(expression)?, span)?;
        let result = self.value(
            ROp::MapConsume { owner, map_type },
            EntityType::Value(map_type),
            span,
        )?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(result))
    }

    fn map_get(&mut self, expression: ExpressionId, span: Span) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .map_get(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let receiver = descriptor.receiver().expression();
        let view = self.narrowed_view(receiver)?;
        let (owner, drop_receiver) = if let Some(view) = view {
            (view, true)
        } else if let Ok(symbol) = self.named_container_symbol(receiver, span) {
            (
                self.borrowed
                    .get(&symbol)
                    .or_else(|| self.bindings.get(&symbol))
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                false,
            )
        } else {
            let Some(slot) = self.required(receiver)? else {
                return Ok(Lowered::Diverged);
            };
            (slot, true)
        };
        let key_expr = descriptor.key().expression();
        let key = if let Some(loan) = self.borrowed_string_loan(key_expr)? {
            loan
        } else {
            let Some(slot) = self.required(key_expr)? else {
                return Ok(Lowered::Diverged);
            };
            slot
        };
        let result_ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let result = self.value(
            ROp::MapGet { owner, key },
            EntityType::Value(result_ty),
            span,
        )?;
        if drop_receiver {
            self.drop_receiver_chain(receiver)?;
        }
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(result))
    }

    fn map_remove(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .map_remove(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let key_expr = descriptor.key().expression();
        let key = if let Some(loan) = self.borrowed_string_loan(key_expr)? {
            loan
        } else {
            let Some(slot) = self.required(key_expr)? else {
                return Ok(Lowered::Diverged);
            };
            slot
        };
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let result_ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let removed = self.body.slot(EntityType::Value(result_ty));
        self.push(
            ROp::MapRemove {
                owner: binding,
                key,
            },
            vec![binding, removed],
            span,
        )?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(removed))
    }

    fn map_with_value(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .map_with_value(id)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let receiver = descriptor.receiver().expression();
        let receiver_span = self.span(receiver);
        let symbol = self.named_container_symbol(receiver, span)?;
        let view = self.narrowed_view(receiver)?;
        let (map_loan, created_map) = if let Some(view) = view {
            (view, None)
        } else if let Some(&loan) = self.borrowed.get(&symbol) {
            (loan, None)
        } else {
            let map_ty = self.expression_type(receiver)?;
            let map_ssa = self.ssa_type(map_ty, receiver_span)?;
            let owner = *self
                .bindings
                .get(&symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, receiver_span))?;
            let place = self.value(
                ROp::RootPlace { owner },
                EntityType::Place(map_ssa),
                receiver_span,
            )?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: map_ssa,
                },
                receiver_span,
            )?;
            self.pending_call_loans.push(loan);
            (loan, Some(loan))
        };
        let key_expr = descriptor.key().expression();
        let key_span = self.span(key_expr);
        let key_ssa = self.ssa_type(self.c(descriptor.key_type()), key_span)?;
        let (key_loan, created_key, key_read) =
            if let Some(loan) = self.borrowed_string_loan(key_expr)? {
                (loan, None, Some(key_expr))
            } else {
                let Some(owner) = self.required(key_expr)? else {
                    return Ok(Lowered::Diverged);
                };
                let place = self.value(
                    ROp::RootPlace { owner },
                    EntityType::Place(key_ssa),
                    key_span,
                )?;
                let loan = self.value(
                    ROp::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: key_ssa,
                    },
                    key_span,
                )?;
                self.pending_call_loans.push(loan);
                (loan, Some(loan), None)
            };
        let action_expr = descriptor.action().expression();
        let action_span = self.span(action_expr);
        let Some((action_loan, created_action, action_read)) =
            self.runtime_initializer_loan(action_expr, action_span)?
        else {
            return Ok(Lowered::Diverged);
        };
        let result_ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let result = self.value(
            ROp::MapWithValue {
                source: map_loan,
                key: key_loan,
                action: action_loan,
            },
            EntityType::Value(result_ty),
            span,
        )?;
        for loan in created_action
            .into_iter()
            .rev()
            .chain(created_key)
            .chain(created_map)
        {
            self.pending_call_loans.retain(|&active| active != loan);
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        }
        for arg in [key_read, action_read].into_iter().flatten() {
            self.drops(UnitDropPoint::AfterExpression(self.unit(arg)))?;
        }
        if view.is_some() {
            self.drop_receiver_chain(receiver)?;
        }
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(result))
    }

    pub(super) fn wrap_map_result(
        &mut self,
        payload: Slot,
        result_ty: SsaTypeId,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let boolean = self
            .inputs
            .typed
            .types()
            .builtin(BuiltinType::Boolean)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let boolean = self.ssa_type(boolean, span)?;
        let present = self.value(
            ROp::Constant(ScalarConstant::Boolean(true)),
            EntityType::Value(boolean),
            span,
        )?;
        self.value(
            ROp::AggregateConstruct {
                aggregate: result_ty,
                fields: vec![present, payload],
            },
            EntityType::Value(result_ty),
            span,
        )
    }

    /// Lowers temporary null comparisons (`m.get(k) == null`, `m.remove(k) != null`) and
    /// scalar `$map.result.tX` equality (`m[k] == value`, `removed == hit`).
    pub(super) fn nullable_equality_binary(
        &mut self,
        expression: ExpressionId,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
        span: Span,
    ) -> Result<Option<Lowered>, LoweringError> {
        if !matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual) {
            return Ok(None);
        }
        let null_operand =
            if crate::ssa::lowering_support::is_null_literal(self.inputs.parsed, right) {
                Some(left)
            } else if crate::ssa::lowering_support::is_null_literal(self.inputs.parsed, left) {
                Some(right)
            } else {
                None
            };
        if let Some(operand) = null_operand {
            let operand_ty = self.expression_type(operand)?;
            if !matches!(
                self.inputs.typed.types().get(operand_ty),
                Some(UnitTypeKind::Nullable(_))
            ) {
                return Ok(None);
            }
            let Some(owner) = self.required(operand)? else {
                return Ok(Some(Lowered::Diverged));
            };
            let nullable_ssa = self.ssa_type(operand_ty, span)?;
            let bool_ty = self.ssa_type(self.expression_type(expression)?, span)?;
            let aggregate_presence = self.types.map_results.contains_key(&nullable_ssa);
            let mut result = if aggregate_presence {
                self.value(
                    ROp::AggregateProject {
                        aggregate: owner,
                        field: 0,
                    },
                    EntityType::Value(bool_ty),
                    span,
                )?
            } else {
                self.value(
                    ROp::NullableIsNull { owner },
                    EntityType::Value(bool_ty),
                    span,
                )?
            };
            if (operator == BinaryOperator::NotEqual) != aggregate_presence {
                result = self.value(
                    ROp::BooleanNot { operand: result },
                    EntityType::Value(bool_ty),
                    span,
                )?;
            }
            self.drops(UnitDropPoint::AfterBinaryOperands(self.unit(expression)))?;
            return Ok(Some(Lowered::Value(result)));
        }
        let left_ty = self.expression_type(left)?;
        let right_ty = self.expression_type(right)?;
        let left_map_res = self.map_result_ssa(left_ty)?;
        let right_map_res = self.map_result_ssa(right_ty)?;
        if left_map_res.is_none() && right_map_res.is_none() {
            return Ok(None);
        }
        let Some(mut left_slot) = self.required(left)? else {
            return Ok(Some(Lowered::Diverged));
        };
        let Some(mut right_slot) = self.required(right)? else {
            return Ok(Some(Lowered::Diverged));
        };
        if let Some(res_ty) = left_map_res
            && right_map_res.is_none()
        {
            right_slot = self.wrap_map_result(right_slot, res_ty, span)?;
        }
        if let Some(res_ty) = right_map_res
            && left_map_res.is_none()
        {
            left_slot = self.wrap_map_result(left_slot, res_ty, span)?;
        }
        let operator = if operator == BinaryOperator::Equal {
            ComparisonOperator::Equal
        } else {
            ComparisonOperator::NotEqual
        };
        let result = self.typed_value(
            ROp::Compare {
                operator,
                left: left_slot,
                right: right_slot,
            },
            expression,
        )?;
        self.drops(UnitDropPoint::AfterBinaryOperands(self.unit(expression)))?;
        Ok(Some(Lowered::Value(result)))
    }

    fn map_result_ssa(&self, ty: UnitTypeId) -> Result<Option<SsaTypeId>, LoweringError> {
        if !matches!(
            self.inputs.typed.types().get(ty),
            Some(UnitTypeKind::Nullable(_))
        ) {
            return Ok(None);
        }
        let ssa = self.ssa_type(ty, self.inputs.span)?;
        Ok(self.types.map_results.contains_key(&ssa).then_some(ssa))
    }

    /// Evaluates a value-form named `null_comparison` on a local binding using
    /// `AggregateProject { field: 0 }` (for `$map.result.tX`) or `NullableIsNull`.
    pub(super) fn map_result_null_comparison_value(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<Lowered>, LoweringError> {
        let id = self.unit(expression);
        let Some(fact) = self.inputs.typed.null_comparison(id) else {
            return Ok(None);
        };
        let Some(&owner) = self.bindings.get(&fact.symbol()) else {
            return Ok(None);
        };
        let nullable_ssa = self.ssa_type(self.c(fact.nullable_type()), span)?;
        if !self.types.map_results.contains_key(&nullable_ssa) {
            return Ok(None);
        }
        let bool_ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let mut result = self.value(
            ROp::AggregateProject {
                aggregate: owner,
                field: 0,
            },
            EntityType::Value(bool_ty),
            span,
        )?;
        if !fact.non_null_when_true() {
            result = self.value(
                ROp::BooleanNot { operand: result },
                EntityType::Value(bool_ty),
                span,
            )?;
        }
        let (Expression::Binary { left, right, .. }, _) = self.node(expression)? else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let mut operand_drops = Vec::new();
        for mut operand in [left, right] {
            loop {
                if let Ok((Expression::Group { expression: inner }, _)) = self.node(operand) {
                    operand_drops.extend(
                        self.collect_drops(UnitDropPoint::AfterExpression(self.unit(operand))),
                    );
                    operand = inner;
                } else {
                    operand_drops.extend(
                        self.collect_drops(UnitDropPoint::AfterExpression(self.unit(operand))),
                    );
                    break;
                }
            }
        }
        operand_drops.extend(self.collect_drops(UnitDropPoint::AfterBinaryOperands(id)));
        self.emit_drops(&operand_drops)?;
        self.finish_drops(&operand_drops);
        Ok(Some(Lowered::Value(result)))
    }
}
