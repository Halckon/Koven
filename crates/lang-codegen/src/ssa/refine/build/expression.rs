//! Expressions in the refined CFG.
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    ownership_checking::UnitDropPoint,
    parser::{
        AssignmentOperator, BinaryOperator, CallArgument, Expression, LiteralKind, PrefixOperator,
        Statement,
    },
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

    fn typed_value(&mut self, op: ROp, expression: ExpressionId) -> Result<Slot, LoweringError> {
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
        if typed.rc_operation(id).is_some() {
            return self.rc_operation(expression, span);
        }
        if let Some(descriptor) = typed.construction(id) {
            return match descriptor.target() {
                lang_frontend::type_checking::UnitConstructionTarget::EnumCase(_) => {
                    self.enum_construction(expression, span)
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
            Expression::This => self.this_value(expression, span).map(Lowered::Value),
            Expression::TypeTest {
                expression: operand,
                negated,
                type_ref,
                ..
            } => self.type_test_expression(operand, type_ref, negated, span),
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand,
                ..
            } => {
                let Some(operand) = self.required(operand)? else {
                    return Ok(Lowered::Diverged);
                };
                Ok(Lowered::Value(
                    self.typed_value(ROp::BooleanNot { operand }, expression)?,
                ))
            }
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
        match self.inputs.typed.types().get(expected) {
            Some(UnitTypeKind::Nullable(inner)) if *inner == actual => {
                let nullable = self.ssa_type(expected, span)?;
                self.deliver(expression);
                self.value(
                    ROp::NullableWrap {
                        nullable,
                        owner: value,
                    },
                    EntityType::Value(nullable),
                    span,
                )
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
            // String operands are read, not consumed; a non-null use reads its proof view.
            let mut operands = Vec::with_capacity(2);
            let mut view_reads = Vec::new();
            for operand in [left, right] {
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
        self.condition(expression, when_true, when_false)?;
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
        self.condition(condition, when_true, when_false)?;
        let mut reached = false;
        for (index, (block, branch)) in [(when_true, Some(then_branch)), (when_false, else_branch)]
            .into_iter()
            .enumerate()
        {
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
            self.goto(join, span)?;
            reached = true;
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
        let receiver = self.receiver_before_arguments(&descriptor, span)?;
        let mut lowered = Vec::with_capacity(arguments.len());
        let mut loans = Vec::new();
        let mut writebacks = Vec::new();
        let mut view_reads = Vec::new();
        let mut delivered = Vec::new();
        for argument in descriptor.arguments() {
            let value_expression = arguments
                .get(argument.argument_index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .value;
            if argument.mode() == ParameterMode::Inout {
                let (loan, writeback) = self.inout_argument(value_expression)?;
                lowered.push(loan);
                if let Some(writeback) = writeback {
                    loans.push(loan);
                    writebacks.push(writeback);
                }
                continue;
            }
            if argument.mode() == ParameterMode::Borrow
                && let Some(&view) = self.inputs.views.get(&self.c(argument.parameter_type()))
            {
                // ADR-0034: a Borrow function parameter receives a callable view.
                let (lent, created) = self.callable_argument(value_expression, view, span)?;
                loans.extend(created);
                lowered.push(lent);
                continue;
            }
            if argument.mode() == ParameterMode::Borrow
                && let Some(view) = self.narrowed_view(value_expression)?
            {
                // A MoveOnly non-null use borrows through the condition's proof view.
                lowered.push(view);
                view_reads.push(value_expression);
                continue;
            }
            let Some(value) = self.required(value_expression)? else {
                return Ok(Lowered::Diverged);
            };
            match argument.mode() {
                ParameterMode::Value => {
                    let value =
                        self.adapt(value, value_expression, self.c(argument.parameter_type()))?;
                    // Delivered at the call: a later argument's jump still drops it.
                    self.repoint_temporary(value_expression, value);
                    delivered.push(value_expression);
                    lowered.push(value);
                }
                ParameterMode::Borrow => {
                    let value = self.adapt_borrowed(value, value_expression, argument)?;
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
        let result = if descriptor.target() == UnitCallTarget::FunctionValue {
            // ADR-0034: a call through a view parameter.
            let Expression::Call { callee, .. } = self.node(expression)?.0 else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let view = self.view_callee(callee)?;
            let op = ROp::CallableInvokeView {
                view,
                arguments: lowered,
            };
            if self.is_unit(self.c(descriptor.return_type())) {
                self.push(op, Vec::new(), span)?;
                Lowered::Unit
            } else {
                Lowered::Value(self.typed_value(op, expression)?)
            }
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
            if self.is_unit(self.c(descriptor.return_type())) {
                self.push(op, Vec::new(), span)?;
                Lowered::Unit
            } else {
                Lowered::Value(self.typed_value(op, expression)?)
            }
        };
        for loan in loans.into_iter().rev() {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        }
        if let Some(loan) = receiver_end {
            self.end_owner_loan(loan, span)?;
        }
        for writeback in writebacks {
            self.write_back(writeback)?;
        }
        self.drops(UnitDropPoint::CallReturn(call))?;
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
        argument: &lang_frontend::type_checking::UnitCallArgumentDescriptor,
    ) -> Result<Slot, LoweringError> {
        let id = self.unit(expression);
        let temporary = self.temporaries.contains_key(&id);
        let adapted = self.adapt(value, expression, self.c(argument.parameter_type()))?;
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
        self.drops(UnitDropPoint::ControlTransfer(self.unit(expression)))?;
        self.end_all_views(span)?;
        let unit_return = self.is_unit(self.inputs.return_type);
        self.finish_return(result, unit_return, span)?;
        Ok(Lowered::Diverged)
    }
}
