//! Expressions in the refined CFG.
mod call;
mod nullable_argument;

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::UnitDropPoint,
    parser::{AssignmentOperator, BinaryOperator, Expression, LiteralKind},
    source::Span,
    type_checking::{UnitTypeId, UnitTypeKind},
};

use super::{Builder, Lowered};
use crate::ssa::{
    integer::{bitwise_operator, checked_operator, comparison_operator},
    lowering_support::{LoweringError, LoweringErrorKind, error, string_literal},
    model::{EntityType, ScalarConstant},
    refine::body::{RAbort, ROp, RTerm, Slot},
};

impl Builder<'_, '_> {
    pub(super) fn expression(
        &mut self,
        expression: ExpressionId,
    ) -> Result<Lowered, LoweringError> {
        let result = self.expression_inner(expression)?;
        if let Lowered::Value(slot) = result {
            self.register_temporary(expression, slot)?;
        }
        if result != Lowered::Diverged {
            self.drops(UnitDropPoint::AfterExpression(self.unit(expression)))?;
        } else {
            let expression = self.unit(expression);
            self.unreachable_drop_points.extend([
                UnitDropPoint::AfterExpression(expression),
                UnitDropPoint::AfterBinaryOperands(expression),
                UnitDropPoint::CallReturn(expression),
            ]);
        }
        Ok(result)
    }

    pub(super) fn required(
        &mut self,
        expression: ExpressionId,
    ) -> Result<Option<Slot>, LoweringError> {
        match self.expression(expression)? {
            Lowered::Value(slot) => Ok(Some(slot)),
            Lowered::Diverged => Ok(None),
            Lowered::Unit => Err(error(LoweringErrorKind::MissingFact, self.span(expression))),
        }
    }

    pub(super) fn typed_value(
        &mut self,
        op: ROp,
        expression: ExpressionId,
    ) -> Result<Slot, LoweringError> {
        let span = self.span(expression);
        let ty = self.ssa_type(self.expression_type(expression)?, span)?;
        self.value(op, EntityType::Value(ty), span)
    }

    pub(super) fn expression_inner(
        &mut self,
        expression: ExpressionId,
    ) -> Result<Lowered, LoweringError> {
        let outer_loans = self.pending_call_loans.len();
        let result = self.build_expression(expression)?;
        if result == Lowered::Diverged {
            // The terminating edge already performed its own cleanup, if any.
            // Loans formed here must not leak into a different control-flow edge.
            self.pending_call_loans.truncate(outer_loans);
        }
        Ok(result)
    }

    fn build_expression(&mut self, expression: ExpressionId) -> Result<Lowered, LoweringError> {
        let (payload, span) = self.node(expression)?;
        let id = self.unit(expression);
        let typed = self.inputs.typed;
        if let Some(value) = self.constant_materialization(expression, span)? {
            return Ok(value);
        }
        if typed.ownership_primitive(id).is_some() {
            return self.ownership_primitive(expression, span);
        }
        if let Some(value) = self.container_operation(expression, span)? {
            return Ok(value);
        }
        if typed.rc_operation(id).is_some() {
            return self.rc_operation(expression, span);
        }
        if typed.integer_operation(id).is_some() {
            return self.integer_operation(expression, span);
        }
        if typed.integer_range_operation(id).is_some() {
            return self.integer_range_operation(expression, span);
        }
        if typed.string_operation(id).is_some() {
            return self.string_operation(expression, span);
        }
        if typed.range_size(id).is_some() {
            return self.lower_range_size(expression, span);
        }
        if let Some(descriptor) = typed.construction(id) {
            return match descriptor.target() {
                lang_frontend::type_checking::UnitConstructionTarget::EnumCase(_) => {
                    self.enum_construction(expression, span)
                }
                lang_frontend::type_checking::UnitConstructionTarget::IntrinsicBox => {
                    self.box_construction(expression, span)
                }
                lang_frontend::type_checking::UnitConstructionTarget::IntrinsicRc => {
                    self.rc_construction(expression, span)
                }
                _ if self.is_value_class(self.c(descriptor.result_type())) => {
                    self.value_construction(expression, span)
                }
                _ => self.construction(expression, span),
            };
        }
        if typed.aggregate_projection(id).is_some() {
            if self.is_enum_projection(expression) {
                return self.enum_projection(expression, span);
            }
            if self.is_value_projection(expression) {
                return self.value_projection(expression, span);
            }
            return self.projection(expression, span);
        }
        if typed.non_null_assertion(id).is_some() {
            return self.non_null_assertion(expression, span);
        }
        match payload {
            Expression::Group { expression: inner } => self.expression(inner),
            Expression::Literal(literal) => self.literal(literal, expression, span),
            Expression::String { .. } => {
                let source_text = self
                    .inputs
                    .sources
                    .source_text(self.inputs.parsed.source_id())
                    .map_err(|_| error(LoweringErrorKind::MismatchedSource, span))?;
                let bytes =
                    string_literal::decode_plain(self.inputs.parsed, source_text, expression)?
                        .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                self.body.temporary_plans.form(id, &bytes, span)?;
                let string = self.ssa_type(self.expression_type(expression)?, span)?;
                Ok(Lowered::Value(self.typed_value(
                    ROp::StringLiteral { string, bytes },
                    expression,
                )?))
            }
            Expression::Name => {
                if self.is_unit(self.expression_type(expression)?) {
                    self.reference(span)?;
                    return Ok(Lowered::Unit);
                }
                self.owner_name(expression, span).map(Lowered::Value)
            }
            // ADR-0034 §4: a returned lambda keeps its concrete callable identity.
            Expression::Lambda { .. } => self.form_callable(expression, span).map(Lowered::Value),
            Expression::This => self.this_value(expression, span).map(Lowered::Value),
            Expression::TypeTest {
                expression: operand,
                negated,
                type_ref,
                ..
            } => self.type_test_expression(operand, type_ref, negated, span),
            Expression::Prefix {
                operator, operand, ..
            } => self.prefix(operator, operand, expression, span),
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => self.binary(expression, left, operator, right, span),
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.if_expression(expression, condition, then_branch, else_branch, span),
            Expression::When {
                subject, entries, ..
            } => self.when_expression(expression, subject, &entries, span),
            Expression::Call { arguments, .. } => self.call(expression, &arguments, span),
            Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                value,
                ..
            } if self.is_field_target(target) => {
                self.field_assignment(expression, target, value, span)
            }
            Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                value,
                ..
            } if self.is_element_target(target) => {
                self.element_assignment(expression, target, value, span)
            }
            Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                ..
            } if self.is_map_put_target(target) => self.map_put(target, span),
            Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                value,
                ..
            } => self.assignment(target, value, span),
            Expression::Assignment {
                target,
                operator:
                    operator @ (AssignmentOperator::AddAssign
                    | AssignmentOperator::SubtractAssign
                    | AssignmentOperator::MultiplyAssign
                    | AssignmentOperator::DivideAssign
                    | AssignmentOperator::RemainderAssign),
                value,
                ..
            } => self.compound_assignment(expression, target, operator, value, span),
            Expression::Return { value, .. } => self.return_expression(expression, value, span),
            Expression::Break { .. } => self.loop_jump(expression, true, span),
            Expression::Continue { .. } => self.loop_jump(expression, false, span),
            _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }

    fn literal(
        &mut self,
        literal: LiteralKind,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        if let LiteralKind::Null = literal {
            let nullable = if let Some(nullable) = self.expected_nullable {
                nullable
            } else {
                self.ssa_type(self.expression_type(expression)?, span)?
            };
            let slot = self.value(
                ROp::NullableNull { nullable },
                EntityType::Value(nullable),
                span,
            )?;
            return Ok(Lowered::Value(slot));
        }
        let ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let op = match literal {
            LiteralKind::Null => unreachable!(),
            LiteralKind::Boolean(value) => ROp::Constant(ScalarConstant::Boolean(value)),
            LiteralKind::Integer(kind) => {
                let text = self
                    .inputs
                    .sources
                    .slice(span)
                    .map_err(|_| error(LoweringErrorKind::MismatchedSource, span))?;
                let value = lang_frontend::type_checking::integer_literal_magnitude(text, kind)
                    .and_then(|value| i128::try_from(value).ok())
                    .ok_or_else(|| error(LoweringErrorKind::InvalidLiteral, span))?;
                ROp::Constant(ScalarConstant::Integer(value))
            }
            LiteralKind::Float(_) | LiteralKind::Char => {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        Ok(Lowered::Value(self.value(
            op,
            EntityType::Value(ty),
            span,
        )?))
    }

    /// A local or parameter read; a proven non-null use reads the narrowed inner value.
    pub(super) fn name(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        if let Some((slot, _)) = self.form_known_function(expression, span)? {
            return Ok(slot);
        }
        let Ok(symbol) = self.reference(span) else {
            let ty = self.expression_type(expression)?;
            if self.is_object_type(ty) {
                let aggregate = self.ssa_type(ty, span)?;
                return self.value(
                    ROp::AggregateConstruct {
                        aggregate,
                        fields: Vec::new(),
                    },
                    EntityType::Value(aggregate),
                    span,
                );
            }
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if let Some(fact) = self.inputs.typed.non_null_use(self.unit(expression)) {
            if fact.symbol() != symbol {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            if let Some(slot) = self.narrowed.get(&symbol).copied() {
                return Ok(slot);
            }
            let narrowed = self.c(fact.narrowed_type());
            if self.move_only(narrowed) && self.active_view(symbol).is_some() {
                if self.c(fact.declared_type()) == self.inputs.return_type {
                    return self
                        .bindings
                        .get(&symbol)
                        .copied()
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span));
                }
                if narrowed == self.inputs.return_type {
                    let owner = self
                        .bindings
                        .get(&symbol)
                        .copied()
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    let proof = self
                        .active_view(symbol)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    let ty = self.ssa_type(narrowed, span)?;
                    self.consume_views_of(symbol);
                    return self.value(
                        ROp::NullableTake { owner, proof },
                        EntityType::Value(ty),
                        span,
                    );
                }
            }
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if let Some(loan) = self.borrowed.get(&symbol).copied() {
            // Copyable Borrow parameter: read the value through the caller's loan.
            let EntityType::Loan { target, .. } = self.body.slots[loan.0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            return self.value(ROp::ReadLoan { loan }, EntityType::Value(target), span);
        }
        self.bindings
            .get(&symbol)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    /// The active proof view for a MoveOnly non-null name use.
    pub(super) fn narrowed_view(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<Slot>, LoweringError> {
        let root = self.ungrouped_expr(expression)?;
        let (payload, span) = self.node(root)?;
        let Expression::Name = payload else {
            return Ok(None);
        };
        let Some(fact) = self.inputs.typed.non_null_use(self.unit(root)) else {
            return Ok(None);
        };
        if !self.move_only(self.c(fact.narrowed_type())) {
            return Ok(None);
        }
        self.active_view(fact.symbol())
            .map(Some)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    /// Applies the frontend-approved `inner -> inner?` adaptation, if any.
    pub(super) fn adapt(
        &mut self,
        value: Slot,
        expression: ExpressionId,
        expected: UnitTypeId,
    ) -> Result<Slot, LoweringError> {
        let span = self.span(expression);
        self.adapt_with_span(value, expression, expected, span)
    }

    pub(super) fn adapt_with_span(
        &mut self,
        value: Slot,
        expression: ExpressionId,
        expected: UnitTypeId,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let actual = self.expression_type(expression)?;
        if actual == expected {
            return Ok(value);
        }
        let expected_ssa = self.ssa_type(expected, span)?;
        if self.body.slots[value.0] == EntityType::Value(expected_ssa) {
            return Ok(value);
        }
        match self.inputs.typed.types().get(expected) {
            Some(UnitTypeKind::Nullable(inner)) if *inner == actual => {
                let nullable = expected_ssa;
                if self.types.map_results.contains_key(&nullable) && !self.move_only(actual) {
                    return self.wrap_map_result(value, nullable, span);
                }
                let wrapped = self.value(
                    ROp::NullableWrap {
                        nullable,
                        owner: value,
                    },
                    EntityType::Value(nullable),
                    span,
                )?;
                self.repoint_temporary(expression, wrapped);
                Ok(wrapped)
            }
            Some(UnitTypeKind::Nullable(_))
                if matches!(
                    self.inputs.typed.types().get(actual),
                    Some(UnitTypeKind::Nullable(inner)) if self.is_nothing(*inner)
                ) =>
            {
                let nullable = expected_ssa;
                let null_slot = self.value(
                    ROp::NullableNull { nullable },
                    EntityType::Value(nullable),
                    span,
                )?;
                self.repoint_temporary(expression, null_slot);
                Ok(null_slot)
            }
            _ => Err(error(LoweringErrorKind::MissingFact, span)),
        }
    }

    fn binary(
        &mut self,
        expression: ExpressionId,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        if matches!(
            operator,
            BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr
        ) || self
            .inputs
            .typed
            .null_comparison(self.unit(expression))
            .is_some()
        {
            return self.condition_value(expression);
        }
        let left_type = self.expression_type(left)?;
        if self.is_nothing(left_type)
            && matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual)
        {
            return match self.expression(left)? {
                Lowered::Diverged => Ok(Lowered::Diverged),
                _ => Err(error(LoweringErrorKind::MissingFact, span)),
            };
        }
        if let Some(lowered) =
            self.nullable_equality_binary(expression, left, operator, right, span)?
        {
            return Ok(lowered);
        }
        let string = matches!(
            self.inputs.typed.types().get(left_type),
            Some(UnitTypeKind::Builtin(
                lang_frontend::type_checking::BuiltinType::String
            ))
        );
        if string {
            // String operands are read, not consumed; a Borrow binding or non-null use reads its loan.
            let mut operands = Vec::with_capacity(2);
            let mut view_reads = Vec::new();
            for operand in [left, right] {
                if let Some(loan) = self.borrowed_string_loan(operand)? {
                    operands.push(loan);
                    view_reads.push(operand);
                    continue;
                }
                if let Some(view) = self.narrowed_view(operand)? {
                    operands.push(view);
                    view_reads.push(operand);
                    continue;
                }
                let Some(slot) = self.required(operand)? else {
                    return Ok(Lowered::Diverged);
                };
                operands.push(slot);
            }
            let (left, right) = (operands[0], operands[1]);
            let op = match operator {
                BinaryOperator::Add => ROp::StringConcat { left, right },
                BinaryOperator::Equal | BinaryOperator::NotEqual => {
                    ROp::StringEqual { left, right }
                }
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            };
            let mut result = self.typed_value(op, expression)?;
            self.drops(UnitDropPoint::AfterBinaryOperands(self.unit(expression)))?;
            // A view read completes at the operation; the name's own drop point follows it.
            for operand in view_reads {
                self.drops(UnitDropPoint::AfterExpression(self.unit(operand)))?;
            }
            if operator == BinaryOperator::NotEqual {
                result = self.typed_value(ROp::BooleanNot { operand: result }, expression)?;
            }
            return Ok(Lowered::Value(result));
        }
        let Some(left) = self.required(left)? else {
            return Ok(Lowered::Diverged);
        };
        let Some(right) = self.required(right)? else {
            return Ok(Lowered::Diverged);
        };
        if let Some(operator) = checked_operator(operator) {
            let ty = self.ssa_type(self.expression_type(expression)?, span)?;
            let boolean = self
                .inputs
                .typed
                .types()
                .builtin(lang_frontend::type_checking::BuiltinType::Boolean)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let boolean = self.ssa_type(boolean, span)?;
            let operation_block = self
                .block
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
            let result = self.body.slot(EntityType::Value(ty));
            let failed = self.body.slot(EntityType::Value(boolean));
            self.push(
                ROp::CheckedArithmetic {
                    operator,
                    left,
                    right,
                },
                vec![result, failed],
                span,
            )?;
            let abort = self.body.block();
            let success = self.body.block();
            self.terminate(
                RTerm::Branch {
                    condition: failed,
                    when_true: abort,
                    when_false: success,
                },
                span,
            )?;
            self.block = Some(abort);
            self.terminate(
                RTerm::Abort(RAbort::CheckedArithmetic {
                    operator,
                    divisor: right,
                }),
                span,
            )?;
            self.block = Some(success);
            self.checked_expression_emitted(
                self.unit(expression),
                [operation_block, success, abort],
                [result, failed, left, right],
                span,
            )?;
            return Ok(Lowered::Value(result));
        }
        if let Some(operator) = bitwise_operator(operator) {
            let result = self.typed_value(
                ROp::IntegerBitwise {
                    operator,
                    left,
                    right,
                },
                expression,
            )?;
            self.drops(UnitDropPoint::AfterBinaryOperands(self.unit(expression)))?;
            return Ok(Lowered::Value(result));
        }
        let operator = comparison_operator(operator)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let result = self.typed_value(
            ROp::Compare {
                operator,
                left,
                right,
            },
            expression,
        )?;
        self.drops(UnitDropPoint::AfterBinaryOperands(self.unit(expression)))?;
        Ok(Lowered::Value(result))
    }

    /// Evaluates a branch-form condition into a Boolean slot.
    fn condition_value(&mut self, expression: ExpressionId) -> Result<Lowered, LoweringError> {
        let span = self.span(expression);
        if let Some(lowered) = self.map_result_null_comparison_value(expression, span)? {
            return Ok(lowered);
        }
        let ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let result = self.body.slot(EntityType::Value(ty));
        let when_true = self.body.block();
        let when_false = self.body.block();
        let join = self.body.block();
        self.condition_with_views(expression, when_true, when_false, false)?;
        for (block, value) in [(when_true, true), (when_false, false)] {
            if !self.body.blocks[block.0].has_predecessor {
                continue;
            }
            self.block = Some(block);
            self.sync_dead();
            self.entry_dead
                .entry(join)
                .or_default()
                .extend(self.dead.iter().copied());
            self.push(
                ROp::Constant(ScalarConstant::Boolean(value)),
                vec![result],
                span,
            )?;
            self.goto(join, span)?;
        }
        if !self.body.blocks[join.0].has_predecessor {
            return Ok(Lowered::Diverged);
        }
        self.block = Some(join);
        self.sync_dead();
        Ok(Lowered::Value(result))
    }
}
