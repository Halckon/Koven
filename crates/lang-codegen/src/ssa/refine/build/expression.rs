//! Expressions in the refined CFG.
#[path = "expression/call.rs"]
mod call;
#[path = "expression/nullable_argument.rs"]
mod nullable_argument;
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::UnitDropPoint,
    parser::{AssignmentOperator, BinaryOperator, CallArgument, Expression, LiteralKind},
    source::Span,
    type_checking::{ParameterMode, UnitCallTarget, UnitTypeId, UnitTypeKind},
};

use super::{Builder, Lowered, View, when::PredecessorBranch};
use crate::ssa::{
    expression_core::parse_float_literal,
    integer::{bitwise_operator, checked_operator, comparison_operator},
    lowering_support::{
        LoweringError, LoweringErrorKind, error, is_float_builtin, parse_char_literal,
        string_literal,
    },
    model::{EntityType, ScalarConstant},
    refine::body::{BlockRef, RAbort, ROp, RTerm, Slot},
};

pub(super) struct ConditionValue {
    pub(super) when_true: BlockRef,
    pub(super) when_false: BlockRef,
    join: BlockRef,
    result: Slot,
    span: Span,
    pending: Vec<View>,
}

impl Builder<'_, '_> {
    pub(super) fn expression(
        &mut self,
        expression: ExpressionId,
    ) -> Result<Lowered, LoweringError> {
        let result = self.expression_inner(expression)?;
        self.finish_expression(expression, result)
    }

    pub(super) fn finish_expression(
        &mut self,
        expression: ExpressionId,
        result: Lowered,
    ) -> Result<Lowered, LoweringError> {
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

    /// Materialize zero-sized storage only when a direct use needs a Unit value.
    pub(super) fn required_unit(
        &mut self,
        expression: ExpressionId,
    ) -> Result<Option<Slot>, LoweringError> {
        match self.expression(expression)? {
            Lowered::Value(slot) => Ok(Some(slot)),
            Lowered::Diverged => Ok(None),
            Lowered::Unit => {
                let root = self.ungrouped_expr(expression)?;
                if let (Expression::Name, span) = self.node(root)?
                    && let Some(slot) = self.bindings.get(&self.reference(span)?)
                {
                    return Ok(Some(*slot));
                }
                self.typed_value(ROp::Constant(ScalarConstant::Unit), expression)
                    .map(Some)
            }
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
        Ok(self.finish_expression_inner(outer_loans, result))
    }

    pub(super) fn finish_expression_inner(
        &mut self,
        outer_loans: usize,
        result: Lowered,
    ) -> Lowered {
        if result == Lowered::Diverged {
            // The terminating edge already performed its own cleanup, if any.
            // Loans formed here must not leak into a different control-flow edge.
            self.pending_call_loans.truncate(outer_loans);
        }
        result
    }

    pub(super) fn special_expression(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<Lowered>, LoweringError> {
        let id = self.unit(expression);
        let typed = self.inputs.typed;
        if let Some(value) = self.constant_materialization(expression, span)? {
            return Ok(Some(value));
        }
        if typed.ownership_primitive(id).is_some() {
            return self.ownership_primitive(expression, span).map(Some);
        }
        if let Some(descriptor) = self.inputs.abandoned_map_require_value(id) {
            return self
                .lower_abandoned_map_require_value(descriptor, span)
                .map(Some);
        }
        if let Some(value) = self.container_operation(expression, span)? {
            return Ok(Some(value));
        }
        if let Some(receiver) = self.inputs.non_completing_projection_receiver(id) {
            return match self.expression(receiver)? {
                Lowered::Diverged => Ok(Some(Lowered::Diverged)),
                _ => Err(error(LoweringErrorKind::InvalidModel, span)),
            };
        }
        if typed.rc_operation(id).is_some() {
            return self.rc_operation(expression, span).map(Some);
        }
        if typed.integer_operation(id).is_some() {
            return self.integer_operation(expression, span).map(Some);
        }
        if typed.integer_range_operation(id).is_some() {
            return self.integer_range_operation(expression, span).map(Some);
        }
        if typed.string_operation(id).is_some() {
            return self.string_operation(expression, span).map(Some);
        }
        if typed.range_size(id).is_some() {
            return self.lower_range_size(expression, span).map(Some);
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
            }
            .map(Some);
        }
        if typed.aggregate_projection(id).is_some() {
            if self.is_enum_projection(expression) {
                return self.enum_projection(expression, span).map(Some);
            }
            if self.is_value_projection(expression) {
                return self.value_projection(expression, span).map(Some);
            }
            return self.projection(expression, span).map(Some);
        }
        if typed.non_null_assertion(id).is_some() {
            return self.non_null_assertion(expression, span).map(Some);
        }
        Ok(None)
    }

    fn build_expression(&mut self, expression: ExpressionId) -> Result<Lowered, LoweringError> {
        let (payload, span) = self.node(expression)?;
        let id = self.unit(expression);
        if let Some(value) = self.special_expression(expression, span)? {
            return Ok(value);
        }
        match payload {
            Expression::Group { expression: inner } => self.expression(inner),
            Expression::Literal(literal) => self.literal(literal, expression, span),
            Expression::String { .. } => {
                if let Some(parts) = self.inputs.typed.interpolation_plan(id) {
                    return self.interpolation(expression, parts, span);
                }
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
            } => self.assignment(expression, target, value, span),
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
            LiteralKind::Char => ROp::Constant(ScalarConstant::Char(parse_char_literal(
                self.inputs.sources,
                span,
            )?)),
            LiteralKind::Float(kind) => {
                ROp::Constant(parse_float_literal(self.inputs.sources, kind, span)?)
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
            if let Some(slot) = self.narrowed_value(symbol) {
                // A later operand can compare the same root and redefine its proof slot.
                return self.value(ROp::Alias { source: slot }, self.body.slots[slot.0], span);
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
        let view = if let Some(witness) = self.inputs.typed.nullable_when_use(self.unit(root)) {
            self.active_views
                .iter()
                .rev()
                .find(|view| {
                    view.symbol == witness.symbol()
                        && view
                            .when_scope
                            .is_some_and(|(control, _)| control == witness.control())
                })
                .map(|view| view.slot)
        } else {
            self.active_view(fact.symbol())
        };
        view.map(Some)
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

    fn borrowed_nullable_string_operand(
        &self,
        operand: ExpressionId,
    ) -> Result<Option<Slot>, LoweringError> {
        let root = self.ungrouped_expr(operand)?;
        let (payload, span) = self.node(root)?;
        if !matches!(payload, Expression::Name) {
            return Ok(None);
        }
        Ok(self
            .reference(span)
            .ok()
            .and_then(|symbol| self.borrowed.get(&symbol).copied()))
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
        let nullable_equality =
            matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual)
                && self
                    .inputs
                    .is_nullable_string_equality(left_type, self.expression_type(right)?);
        if string || nullable_equality {
            let presence = crate::ssa::lowering_support::nullable_string_presence_on_left(
                self.inputs.typed,
                left_type,
                self.expression_type(right)?,
            );
            // String operands are read, not consumed; a Borrow binding or non-null use reads its loan.
            let mut operands = Vec::with_capacity(2);
            let mut view_reads = Vec::new();
            let mut field_presence = None;
            let loan_boundary = self.pending_call_loans.len();
            for operand in [left, right] {
                if matches!(
                    self.inputs.shared_field_operand_target(expression, operand),
                    Some(
                        lang_frontend::ownership_checking::UnitLoanTarget::Place(_)
                            | lang_frontend::ownership_checking::UnitLoanTarget::Temporary(_)
                    )
                ) {
                    let operand_span = self.span(operand);
                    let (loan, created, receivers) = self
                        .borrowed_string_field_loan(
                            operand,
                            self.expression_type(operand)?,
                            operand_span,
                        )?
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, operand_span))?;
                    if presence.is_some() {
                        // Copy the field's tag while evaluating this operand. The other
                        // operand may legally write a disjoint field on the same owner.
                        field_presence = Some(
                            self.typed_value(ROp::NullableLoanIsNull { source: loan }, expression)?,
                        );
                        for loan in created.into_iter().rev() {
                            self.end_short_call_loan(loan, span)?;
                        }
                    } else {
                        self.pending_call_loans.extend(created);
                    }
                    operands.push(loan);
                    view_reads.extend(receivers);
                    view_reads.push(operand);
                    continue;
                }
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
                if self
                    .inputs
                    .is_nullable_string(self.expression_type(operand)?)
                    && let Some(loan) = self.borrowed_nullable_string_operand(operand)?
                {
                    operands.push(loan);
                    view_reads.push(operand);
                    continue;
                }
                let Some(slot) = self.required(operand)? else {
                    return Ok(Lowered::Diverged);
                };
                operands.push(slot);
            }
            let (left, right) = (operands[0], operands[1]);
            let mut result = if let Some(value) = field_presence {
                value
            } else if let Some(on_left) = presence {
                let slot = if on_left { left } else { right };
                let op = match self.body.slots[slot.0] {
                    EntityType::Value(_) => ROp::NullableIsNull { owner: slot },
                    EntityType::Loan { .. } => ROp::NullableLoanIsNull { source: slot },
                    EntityType::Place(_) => {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                };
                self.typed_value(op, expression)?
            } else {
                let op = match operator {
                    BinaryOperator::Add => ROp::StringConcat { left, right },
                    BinaryOperator::Equal | BinaryOperator::NotEqual => {
                        ROp::StringEqual { left, right }
                    }
                    _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
                };
                self.typed_value(op, expression)?
            };
            // Only loans created by this operation end here. Early exits use
            // the same pending-loan cleanup as an unfinished call argument.
            for loan in self
                .pending_call_loans
                .split_off(loan_boundary)
                .into_iter()
                .rev()
            {
                self.end_short_call_loan(loan, span)?;
            }
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
            let ty = self.expression_type(expression)?;
            if matches!(self.inputs.typed.types().get(ty), Some(UnitTypeKind::Builtin(builtin)) if is_float_builtin(*builtin))
            {
                let result = self.typed_value(
                    ROp::FloatArithmetic {
                        operator: operator.into(),
                        left,
                        right: Some(right),
                    },
                    expression,
                )?;
                self.drops(UnitDropPoint::AfterBinaryOperands(self.unit(expression)))?;
                return Ok(Lowered::Value(result));
            }
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
        if let Some(lowered) = self.named_null_comparison_value(expression, span)? {
            return Ok(lowered);
        }
        let value = self.prepare_condition_value(expression, span)?;
        self.condition_with_views(expression, value.when_true, value.when_false, false)?;
        self.finish_condition_value(value)
    }

    pub(super) fn prepare_condition_value(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<ConditionValue, LoweringError> {
        let ty = self.ssa_type(self.expression_type(expression)?, span)?;
        Ok(ConditionValue {
            result: self.body.slot(EntityType::Value(ty)),
            when_true: self.body.block(),
            when_false: self.body.block(),
            join: self.body.block(),
            pending: std::mem::take(&mut self.pending_views),
            span,
        })
    }

    pub(super) fn finish_condition_value(
        &mut self,
        value: ConditionValue,
    ) -> Result<Lowered, LoweringError> {
        let ConditionValue {
            when_true,
            when_false,
            join,
            result,
            pending,
            span,
        } = value;
        let inherited = self.active_views.clone();
        let mut predecessors = Vec::new();
        for (block, value) in [(when_true, true), (when_false, false)] {
            if !self.body.blocks[block.0].has_predecessor {
                continue;
            }
            self.active_views.clone_from(&inherited);
            self.enter(block);
            self.push(
                ROp::Constant(ScalarConstant::Boolean(value)),
                vec![result],
                span,
            )?;
            predecessors.push(PredecessorBranch {
                block,
                views: self.active_views.clone(),
                dead: self.dead.clone(),
                this_dead: self.this_dead,
            });
        }
        self.pending_views = pending;
        if !self.join_views(predecessors, join, span)? {
            return Ok(Lowered::Diverged);
        }
        self.sync_dead();
        Ok(Lowered::Value(result))
    }
}
