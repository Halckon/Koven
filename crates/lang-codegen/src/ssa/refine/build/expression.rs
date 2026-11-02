//! Expressions in the refined CFG.
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    ownership_checking::UnitDropPoint,
    parser::{
        AssignmentOperator, BinaryOperator, CallArgument, Expression, LiteralKind, Statement,
    },
    source::Span,
    type_checking::{ParameterMode, UnitCallTarget, UnitTypeId, UnitTypeKind},
};

use super::{Builder, Lowered, View};
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
            Expression::Name => self.name(expression, span).map(Lowered::Value),
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
    fn name(&mut self, expression: ExpressionId, span: Span) -> Result<Slot, LoweringError> {
        let symbol = self.reference(span)?;
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
        let (payload, span) = self.node(expression)?;
        let Expression::Name = payload else {
            return Ok(None);
        };
        let Some(fact) = self.inputs.typed.non_null_use(self.unit(expression)) else {
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
        let ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let result = self.body.slot(EntityType::Value(ty));
        let when_true = self.body.block();
        let when_false = self.body.block();
        let join = self.body.block();
        self.condition_with_views(expression, when_true, when_false, false)?;
        for (block, value) in [(when_true, true), (when_false, false)] {
            self.block = Some(block);
            self.push(
                ROp::Constant(ScalarConstant::Boolean(value)),
                vec![result],
                span,
            )?;
            self.goto(join, span)?;
        }
        self.block = Some(join);
        Ok(Lowered::Value(result))
    }

    fn if_expression(
        &mut self,
        expression: ExpressionId,
        condition: ExpressionId,
        then_branch: StatementId,
        else_branch: Option<StatementId>,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let control = self.unit(expression);
        let ty = self.expression_type(expression)?;
        let result = if self.is_unit(ty) || self.is_nothing(ty) {
            None
        } else {
            Some(self.body.slot(EntityType::Value(self.ssa_type(ty, span)?)))
        };
        let when_true = self.body.block();
        let when_false = self.body.block();
        let join = self.body.block();
        self.condition_for_control(condition, when_true, when_false, span)?;
        let outer_views = self.active_views.clone();
        let outer_pending_loans = self.pending_call_loans.clone();
        let mut branch_views = Vec::new();
        let mut reached = false;
        for (index, (block, branch)) in [(when_true, Some(then_branch)), (when_false, else_branch)]
            .into_iter()
            .enumerate()
        {
            self.active_views = outer_views.clone();
            self.pending_call_loans = outer_pending_loans.clone();
            let mark = self.enter(block);
            let value = match branch {
                Some(branch) => self.branch_value(branch)?,
                None => Some(None),
            };
            self.leave(mark, span)?;
            let Some(value) = value else {
                continue;
            };
            if let Some(result) = result {
                let (slot, source) =
                    value.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let slot = self.adapt(slot, source, ty)?;
                self.deliver(source);
                self.push(ROp::Alias { source: slot }, vec![result], span)?;
            }
            self.drops(UnitDropPoint::BranchExit {
                control,
                branch: index,
            })?;
            branch_views.push(self.active_views.clone());
            self.goto(join, span)?;
            reached = true;
        }
        self.pending_call_loans = outer_pending_loans;
        if branch_views.len() == 2 {
            let common: Vec<View> = branch_views[0]
                .iter()
                .filter(|v| branch_views[1].iter().any(|other| other.slot == v.slot))
                .copied()
                .collect();
            self.active_views = common;
        } else if let Some(first) = branch_views.pop() {
            self.active_views = first;
        }
        if !self.pending_views.is_empty() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        if !reached {
            self.block = None;
            return Ok(Lowered::Diverged);
        }
        self.block = Some(join);
        Ok(result.map_or(Lowered::Unit, Lowered::Value))
    }

    /// Lowers a branch body; `None` when it diverges, else its tail value and expression.
    pub(super) fn branch_value(
        &mut self,
        branch: StatementId,
    ) -> Result<Option<Option<(Slot, ExpressionId)>>, LoweringError> {
        let node = self
            .inputs
            .parsed
            .ast()
            .statements()
            .get(branch)
            .map_err(|_| error(LoweringErrorKind::MissingFact, self.inputs.span))?;
        let tail =
            match node.payload() {
                Statement::Expression { expression } => Some(*expression),
                Statement::ControlBody { elements } => elements.last().and_then(|last| match self
                    .inputs
                    .parsed
                    .ast()
                    .statements()
                    .get(*last)
                    .map(|n| n.payload())
                {
                    Ok(Statement::Expression { expression }) => Some(*expression),
                    _ => None,
                }),
                _ => None,
            };
        match self.statement(branch)? {
            Lowered::Diverged => Ok(None),
            Lowered::Value(slot) => Ok(Some(tail.map(|tail| (slot, tail)))),
            Lowered::Unit => Ok(Some(None)),
        }
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
            self.receiver_before_arguments(&descriptor, span)?
        };
        let mut scoped_receiver_loan = None;
        if let Some(super::receiver::Receiver::Ready {
            loan: Some(ref loan),
            ..
        }) = receiver
            && loan.scoped
        {
            self.pending_call_loans.push(loan.loan);
            scoped_receiver_loan = Some(loan.loan);
        }
        let mut call_args = Vec::with_capacity(arguments.len() + usize::from(is_extension));
        if is_extension
            && let Some(rec) = descriptor.receiver()
            && let lang_frontend::type_checking::UnitCallReceiverOrigin::Expression(expression) =
                rec.origin()
            && expression.source_unit() == self.inputs.source_unit
        {
            call_args.push((expression.expression(), rec.mode(), rec.ty()));
        }
        for argument in descriptor.arguments() {
            let value_expression = arguments
                .get(argument.argument_index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .value;
            call_args.push((value_expression, argument.mode(), argument.parameter_type()));
        }
        let mut lowered = Vec::with_capacity(call_args.len());
        let mut loans = Vec::new();
        let mut writebacks = Vec::new();
        let mut view_reads = Vec::new();
        let mut delivered = Vec::new();
        for (value_expression, mode, parameter_type) in call_args {
            if mode == ParameterMode::Inout {
                let (loan, writeback) = self.inout_argument(value_expression)?;
                lowered.push(loan);
                if let Some(writeback) = writeback {
                    loans.push(loan);
                    self.pending_call_loans.push(loan);
                    writebacks.push(writeback);
                }
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
                lowered.push(lent);
                view_reads.extend(read_argument);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some(loan) =
                    self.borrowed_move_only_loan(value_expression, self.c(parameter_type))?
            {
                lowered.push(loan);
                view_reads.push(value_expression);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some((loan, created, field_receiver)) =
                    self.borrowed_string_field_loan(value_expression, self.c(parameter_type), span)?
            {
                loans.extend(created.iter().copied());
                self.pending_call_loans.extend(created);
                lowered.push(loan);
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
                lowered.push(loan);
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
                lowered.push(loan);
                view_reads.extend(elem_arg);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some(view) = self.narrowed_view(value_expression)?
            {
                // A MoveOnly non-null use borrows through the condition's proof view.
                lowered.push(view);
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
                    lowered.push(loan);
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
                    lowered.push(value);
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
                        argument_span,
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
                    lowered.push(loan);
                }
                ParameterMode::Inout => {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                }
            }
        }
        for value_expression in delivered {
            self.deliver(value_expression);
        }
        let mut receiver_end = None;
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
            let (receiver, receiver_loan) = match receiver {
                Some(receiver) => {
                    let (operand, loan) = self.receiver_at_call(receiver, span)?;
                    (Some(operand), loan)
                }
                None => (None, None),
            };
            receiver_end = receiver_loan;
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
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
            self.pending_call_loans.retain(|&active| active != loan);
        }
        if let Some(loan) = receiver_end {
            self.end_owner_loan(loan, span)?;
            if scoped_receiver_loan.is_some() {
                self.pending_call_loans
                    .retain(|&active| active != loan.loan);
            }
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

    fn snapshot_binding_if_name(
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
            && self.bindings.get(&symbol).copied() == Some(slot)
        {
            let snapshot = self.body.slot(self.body.slots[slot.0]);
            self.push(ROp::Alias { source: slot }, vec![snapshot], span)?;
            return Ok(snapshot);
        }
        Ok(slot)
    }
}
