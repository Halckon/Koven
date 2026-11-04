//! Expressions in the refined CFG.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::UnitDropPoint,
    parser::{AssignmentOperator, BinaryOperator, CallArgument, Expression, LiteralKind},
    source::Span,
    type_checking::{ParameterMode, UnitCallTarget, UnitTypeId, UnitTypeKind},
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error, string_literal},
    model::{CheckedArithmeticOperator, ComparisonOperator, EntityType, LoanKind, ScalarConstant},
    refine::body::{RAbort, ROp, RTerm, Slot},
};

pub(super) fn checked_operator(operator: BinaryOperator) -> Option<CheckedArithmeticOperator> {
    match operator {
        BinaryOperator::Add => Some(CheckedArithmeticOperator::Add),
        BinaryOperator::Subtract => Some(CheckedArithmeticOperator::Subtract),
        BinaryOperator::Multiply => Some(CheckedArithmeticOperator::Multiply),
        BinaryOperator::Divide => Some(CheckedArithmeticOperator::Divide),
        BinaryOperator::Remainder => Some(CheckedArithmeticOperator::Remainder),
        _ => None,
    }
}

pub(super) fn comparison_operator(operator: BinaryOperator) -> Option<ComparisonOperator> {
    match operator {
        BinaryOperator::Equal => Some(ComparisonOperator::Equal),
        BinaryOperator::NotEqual => Some(ComparisonOperator::NotEqual),
        BinaryOperator::Less => Some(ComparisonOperator::LessThan),
        BinaryOperator::LessEqual => Some(ComparisonOperator::LessThanOrEqual),
        BinaryOperator::Greater => Some(ComparisonOperator::GreaterThan),
        BinaryOperator::GreaterEqual => Some(ComparisonOperator::GreaterThanOrEqual),
        _ => None,
    }
}

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
                self.name(expression, span).map(Lowered::Value)
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
        let ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let op = match literal {
            LiteralKind::Null => ROp::NullableNull { nullable: ty },
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
            return self
                .narrowed
                .get(&symbol)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span));
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
        if let Some(lowered) =
            self.nullable_equality_binary(expression, left, operator, right, span)?
        {
            return Ok(lowered);
        }
        let string = matches!(
            self.inputs.typed.types().get(self.expression_type(left)?),
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
            return Ok(Lowered::Value(result));
        }
        if let Some(operator) = crate::ssa::integer::bitwise_operator(operator) {
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
        self.block = Some(join);
        self.sync_dead();
        Ok(Lowered::Value(result))
    }

    fn call(
        &mut self,
        expression: ExpressionId,
        arguments: &[CallArgument],
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let call = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .call(call)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            .clone();
        if descriptor.arguments().len() != arguments.len() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let is_extension = match descriptor.target() {
            UnitCallTarget::Declaration(id) => self
                .inputs
                .typed
                .signatures()
                .declaration(id)
                .and_then(|signature| signature.callable())
                .is_some_and(|callable| {
                    callable.range_extension().is_some() || callable.text_extension().is_some()
                }),
            _ => false,
        };
        let receiver = if is_extension {
            None
        } else {
            self.receiver_before_arguments(call, &descriptor, span)?
        };
        if let Some(super::receiver::Receiver::Ready { ref loans, .. }) = receiver {
            self.pending_call_loans.extend(loans.iter().copied());
        }
        let ext_offset = usize::from(is_extension);
        let mut call_args = Vec::with_capacity(arguments.len() + ext_offset);
        if is_extension
            && let Some(rec) = descriptor.receiver()
            && let lang_frontend::type_checking::UnitCallReceiverOrigin::Expression(expression) =
                rec.origin()
            && expression.source_unit() == self.inputs.source_unit
        {
            call_args.push((
                0,
                expression.expression(),
                self.span(expression.expression()),
                rec.mode(),
                rec.ty(),
            ));
        }
        let mut sorted_args: Vec<_> = descriptor.arguments().iter().collect();
        sorted_args.sort_by_key(|argument| argument.argument_index());
        for argument in sorted_args {
            let call_argument = arguments
                .get(argument.argument_index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            call_args.push((
                argument.parameter_index() + ext_offset,
                call_argument.value,
                call_argument.span,
                argument.mode(),
                argument.parameter_type(),
            ));
        }
        let mut lowered_slots: Vec<Option<Slot>> = vec![None; call_args.len()];
        let mut loans = Vec::new();
        let mut writebacks = Vec::new();
        let mut view_reads = Vec::new();
        let mut delivered = Vec::new();
        for (param_slot, value_expression, call_arg_span, mode, parameter_type) in call_args {
            if mode == ParameterMode::Inout {
                let (loan, writeback) = self.inout_argument(value_expression)?;
                lowered_slots[param_slot] = Some(loan);
                if let Some(writeback) = writeback {
                    loans.push(loan);
                    self.pending_call_loans.push(loan);
                    writebacks.push(writeback);
                }
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some((loan, created)) =
                    self.lower_nested_borrow_argument(call, value_expression, call_arg_span)?
            {
                loans.extend(created.iter().copied());
                loans.push(loan);
                self.pending_call_loans.extend(created);
                self.pending_call_loans.push(loan);
                lowered_slots[param_slot] = Some(loan);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some((loan, created, read_expr)) = self.lower_short_range_argument(
                    value_expression,
                    self.c(parameter_type),
                    call_arg_span,
                )?
            {
                loans.extend(created.iter().copied());
                self.pending_call_loans.extend(created);
                lowered_slots[param_slot] = Some(loan);
                view_reads.extend(read_expr);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some(&view) = self.inputs.views.get(&self.c(parameter_type))
            {
                // ADR-0034: a Borrow function parameter receives a callable view.
                let (lent, created, read_argument) =
                    self.callable_argument(value_expression, view, span)?;
                loans.extend(created.iter().copied());
                self.pending_call_loans.extend(created);
                lowered_slots[param_slot] = Some(lent);
                view_reads.extend(read_argument);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some(loan) =
                    self.borrowed_move_only_loan(value_expression, self.c(parameter_type))?
            {
                lowered_slots[param_slot] = Some(loan);
                view_reads.push(value_expression);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some((loan, created, field_receiver)) =
                    self.borrowed_string_field_loan(value_expression, self.c(parameter_type), span)?
            {
                loans.extend(created.iter().copied());
                self.pending_call_loans.extend(created);
                lowered_slots[param_slot] = Some(loan);
                view_reads.extend(field_receiver);
                view_reads.push(value_expression);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some((loan, created, rc_receiver)) =
                    self.borrowed_rc_payload_loan(value_expression, self.c(parameter_type), span)?
            {
                loans.extend(created.iter().copied());
                self.pending_call_loans.extend(created);
                lowered_slots[param_slot] = Some(loan);
                view_reads.extend(rc_receiver);
                view_reads.push(value_expression);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some((loan, created, elem_arg)) = self.borrowed_container_element_loan(
                    value_expression,
                    self.c(parameter_type),
                    span,
                )?
            {
                loans.extend(created.iter().copied());
                self.pending_call_loans.extend(created);
                lowered_slots[param_slot] = Some(loan);
                view_reads.extend(elem_arg);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some(view) = self.narrowed_view(value_expression)?
            {
                // A MoveOnly non-null use borrows through the condition's proof view.
                lowered_slots[param_slot] = Some(view);
                view_reads.push(value_expression);
                continue;
            }
            if mode == ParameterMode::Borrow {
                let mut current = value_expression;
                let mut source_loan = None;
                while let Ok(node) = self.inputs.parsed.ast().expressions().get(current) {
                    match node.payload() {
                        Expression::Group { expression } => current = *expression,
                        Expression::Name => {
                            if let Ok(symbol) = self.reference(self.span(current)) {
                                source_loan = self.borrowed.get(&symbol).copied();
                            }
                            break;
                        }
                        _ => break,
                    }
                }
                if let Some(source_loan) = source_loan {
                    let argument_span = self.span(value_expression);
                    let EntityType::Loan { kind, target } = self.body.slots[source_loan.0] else {
                        return Err(error(LoweringErrorKind::InvalidModel, argument_span));
                    };
                    let loan = self.value(
                        ROp::SharedReborrow {
                            source: source_loan,
                        },
                        EntityType::Loan { kind, target },
                        argument_span,
                    )?;
                    loans.push(loan);
                    self.pending_call_loans.push(loan);
                    lowered_slots[param_slot] = Some(loan);
                    continue;
                }
            }
            let Some(value) = self.required(value_expression)? else {
                return Ok(Lowered::Diverged);
            };
            match mode {
                ParameterMode::Value => {
                    let mut value = self.adapt(value, value_expression, self.c(parameter_type))?;
                    value = self.snapshot_binding_if_name(value_expression, value)?;
                    // Delivered at the call: a later argument's jump still drops it.
                    self.repoint_temporary(value_expression, value);
                    delivered.push(value_expression);
                    lowered_slots[param_slot] = Some(value);
                }
                ParameterMode::Borrow => {
                    let value = self.adapt_borrowed(value, value_expression, parameter_type)?;
                    let target = self.body.slots[value.0];
                    let EntityType::Value(target) = target else {
                        return Err(error(LoweringErrorKind::InvalidModel, span));
                    };
                    // The loan originates at the borrowed argument; it ends at the call.
                    let argument_span = self.span(value_expression);
                    let place = self.value(
                        ROp::RootPlace { owner: value },
                        EntityType::Place(target),
                        call_arg_span,
                    )?;
                    let loan = self.value(
                        ROp::BorrowBegin {
                            place,
                            kind: LoanKind::Shared,
                        },
                        EntityType::Loan {
                            kind: LoanKind::Shared,
                            target,
                        },
                        argument_span,
                    )?;
                    loans.push(loan);
                    self.pending_call_loans.push(loan);
                    lowered_slots[param_slot] = Some(loan);
                }
                ParameterMode::Inout => {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                }
            }
        }
        let lowered = lowered_slots
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        for value_expression in delivered {
            self.deliver(value_expression);
        }
        let mut receiver_loans = Vec::new();
        let mut invoked_callee = None;
        let unit_return = self.is_unit(self.c(descriptor.return_type()));
        let result = if descriptor.target() == UnitCallTarget::FunctionValue {
            let (result, callee) = self.invoke_callable(expression, lowered, unit_return, span)?;
            invoked_callee = callee;
            result
        } else if descriptor.aborts() {
            let [message] = lowered.as_slice() else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            self.terminate(RTerm::Abort(RAbort::Error { message: *message }), span)?;
            return Ok(Lowered::Diverged);
        } else if let Some(kind) = descriptor.text_primitive() {
            self.text_primitive_call(expression, kind, &lowered, span)?
        } else if descriptor.prints_line() {
            let [loan] = lowered.as_slice() else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            self.push(ROp::PrintString { value: *loan }, Vec::new(), span)?;
            Lowered::Unit
        } else {
            if !matches!(
                descriptor.target(),
                UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
            ) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let resolved = self
                .inputs
                .source_plan
                .call_site(self.inputs.source_token, call)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let callee = self
                .inputs
                .function_ids
                .get(resolved.key())
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let (receiver, r_loans) = match receiver {
                Some(receiver) => {
                    let (operand, r_loans, r_wb) = self.receiver_at_call(receiver, span)?;
                    writebacks.extend(r_wb);
                    (Some(operand), r_loans)
                }
                None => (None, Vec::new()),
            };
            receiver_loans = r_loans;
            let op = ROp::DirectCall {
                callee,
                receiver,
                arguments: lowered,
            };
            if unit_return {
                self.push(op, Vec::new(), span)?;
                Lowered::Unit
            } else if let Some(&callable) = self.inputs.callable_returns.get(&callee) {
                Lowered::Value(self.value(op, EntityType::Value(callable), span)?)
            } else {
                Lowered::Value(self.typed_value(op, expression)?)
            }
        };
        for loan in loans.into_iter().rev() {
            self.end_short_call_loan(loan, span)?;
            self.pending_call_loans.retain(|&active| active != loan);
        }
        for loan in receiver_loans.into_iter().rev() {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
            self.pending_call_loans.retain(|&active| active != loan);
        }
        for writeback in writebacks {
            self.write_back(writeback)?;
        }
        if let Some(callee) = invoked_callee {
            self.drops(UnitDropPoint::AfterExpression(self.unit(callee)))?;
        }
        self.drops(UnitDropPoint::CallReturn(call))?;
        if let Some(receiver_desc) = descriptor.receiver()
            && receiver_desc.mode() != ParameterMode::Value
            && let lang_frontend::type_checking::UnitCallReceiverOrigin::Expression(receiver_expr) =
                receiver_desc.origin()
            && receiver_expr.source_unit() == self.inputs.source_unit
        {
            self.drops(UnitDropPoint::AfterExpression(receiver_expr))?;
        }
        for argument in view_reads {
            self.drops(UnitDropPoint::AfterExpression(self.unit(argument)))?;
        }
        Ok(result)
    }

    /// A Borrow argument of type `T` for a `T?` parameter: the temporary is wrapped and
    /// its drop obligation moves to the wrapper.
    fn adapt_borrowed(
        &mut self,
        value: Slot,
        expression: ExpressionId,
        parameter_type: UnitTypeId,
    ) -> Result<Slot, LoweringError> {
        let id = self.unit(expression);
        let temporary = self.temporaries.contains_key(&id);
        let adapted = self.adapt(value, expression, self.c(parameter_type))?;
        if adapted != value && temporary {
            self.temporaries.insert(id, adapted);
        }
        Ok(adapted)
    }

    fn assignment(
        &mut self,
        target: ExpressionId,
        value: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let symbol = self.reference(self.span(target))?;
        let Some(slot) = self.required(value)? else {
            return Ok(Lowered::Diverged);
        };
        let declared = self
            .symbol_type(symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let slot = self.adapt(slot, value, declared)?;
        self.deliver(value);
        if let Some(loan) = self.inout_loan(symbol) {
            // The caller's place now holds the new value; nothing is rebound here.
            self.push(ROp::LoanMutate { loan, value: slot }, Vec::new(), span)?;
            return Ok(Lowered::Unit);
        }
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.end_views_of(symbol, span)?;
        self.sync_dead();
        self.dead.remove(&symbol);
        self.push(ROp::Alias { source: slot }, vec![binding], span)?;
        self.narrowed.remove(&symbol);
        Ok(Lowered::Unit)
    }

    fn return_expression(
        &mut self,
        expression: ExpressionId,
        value: Option<ExpressionId>,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        if self.inputs.borrow_return.is_some() {
            let value = value.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            return self.lower_borrow_return(expression, value, span);
        }
        if self.inputs.carrier_return.is_some() {
            let value = value.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            return self.lower_carrier_return(expression, value, span);
        }
        let result = match value {
            Some(value) => {
                let Some(slot) = self.required(value)? else {
                    return Ok(Lowered::Diverged);
                };
                let slot = self.adapt(slot, value, self.inputs.return_type)?;
                self.deliver(value);
                Some(slot)
            }
            None => None,
        };
        for loan in std::mem::take(&mut self.pending_call_loans)
            .into_iter()
            .rev()
        {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        }
        self.end_all_views(span)?;
        self.drops(UnitDropPoint::ControlTransfer(self.unit(expression)))?;
        let unit_return = self.is_unit(self.inputs.return_type);
        self.finish_return(result, unit_return, span)?;
        Ok(Lowered::Diverged)
    }

    pub(super) fn snapshot_binding_if_name(
        &mut self,
        expression: ExpressionId,
        slot: Slot,
    ) -> Result<Slot, LoweringError> {
        let mut target = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(target)? {
            target = inner;
        }
        if let (Expression::Name, span) = self.node(target)?
            && let Ok(symbol) = self.reference(span)
            && self
                .symbol_type(symbol)
                .is_some_and(|ty| !self.move_only(ty))
            && self.bindings.get(&symbol).copied() == Some(slot)
        {
            let snapshot = self.body.slot(self.body.slots[slot.0]);
            self.push(ROp::Alias { source: slot }, vec![snapshot], span)?;
            return Ok(snapshot);
        }
        Ok(slot)
    }
}
