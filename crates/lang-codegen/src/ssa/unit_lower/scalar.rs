//! compilation-unit scalar prefix、checked arithmetic 与 comparison lowering。

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::UnitDropPoint,
    parser::{BinaryOperator, Expression, LiteralKind, PrefixOperator},
    source::Span,
    type_checking::{BuiltinType, UnitExpressionId},
};

use super::{
    LoweredValue, UnitExpressionLowerer, builtin_type, lowering_error, parse_integer_literal,
    require_value, resolve_concrete_type,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{
        CheckedArithmeticOperator, ComparisonOperator, EntityType, Operation, Origin,
        ScalarConstant, SsaTypeId, TerminatorKind, ValueId,
    },
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_prefix(
        &mut self,
        operator: PrefixOperator,
        operand: ExpressionId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let operand_type = self.expression_builtin_type(operand, span)?;
        let supported = match operator {
            PrefixOperator::Not => operand_type == Some(BuiltinType::Boolean),
            PrefixOperator::Plus | PrefixOperator::Minus => {
                operand_type.is_some_and(is_integer_builtin)
            }
        };
        if !supported {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if operator == PrefixOperator::Minus {
            let operand_node = self
                .parsed
                .ast()
                .expressions()
                .get(operand)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if let Expression::Literal(LiteralKind::Integer(kind)) = operand_node.payload() {
                let constant = parse_integer_literal(self.sources, *kind, operand_node.span())?
                    .checked_neg()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidLiteral, span))?;
                let ty = self.expression_ssa_type(expression, span)?;
                return self.append_scalar(
                    Operation::Constant(ScalarConstant::Integer(constant)),
                    ty,
                    span,
                );
            }
        }

        let operand = self.lower_required_value(operand, span)?;
        match operator {
            PrefixOperator::Plus => Ok(LoweredValue::Value(operand)),
            PrefixOperator::Not => {
                let ty = self.expression_ssa_type(expression, span)?;
                self.append_scalar(Operation::BooleanNot { operand }, ty, span)
            }
            PrefixOperator::Minus => {
                let ty = self.expression_ssa_type(expression, span)?;
                let zero =
                    self.append_scalar(Operation::Constant(ScalarConstant::Integer(0)), ty, span)?;
                let LoweredValue::Value(zero) = zero else {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                };
                self.checked(CheckedArithmeticOperator::Subtract, zero, operand, ty, span)
            }
        }
    }

    pub(super) fn lower_scalar_binary(
        &mut self,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if matches!(
            operator,
            BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr
        ) {
            return self.lower_short_circuit(left, operator, right, expression, span);
        }
        let operand_type = self.expression_builtin_type(left, span)?;
        if operand_type == Some(BuiltinType::String) {
            return self.lower_string_binary(left, operator, right, expression, span);
        }
        let supported = match operator {
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Remainder
            | BinaryOperator::BitwiseAnd
            | BinaryOperator::BitwiseOr
            | BinaryOperator::BitwiseXor
            | BinaryOperator::Shl
            | BinaryOperator::Shr
            | BinaryOperator::Ushr
            | BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual => operand_type.is_some_and(is_integer_builtin),
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                matches!(operand_type, Some(BuiltinType::Boolean | BuiltinType::Char))
                    || operand_type.is_some_and(is_integer_builtin)
            }
            _ => false,
        };
        if !supported {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let left = self.lower_required_value(left, span)?;
        let right = self.lower_required_value(right, span)?;
        let ty = self.expression_ssa_type(expression, span)?;
        if let Some(operator) = checked_operator(operator) {
            return self.checked(operator, left, right, ty, span);
        }
        if let Some(operator) = crate::ssa::integer::bitwise_operator(operator) {
            return self.append_scalar(
                Operation::IntegerBitwise {
                    operator,
                    left,
                    right,
                },
                ty,
                span,
            );
        }
        let operator = comparison_operator(operator)
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        self.append_scalar(
            Operation::Compare {
                operator,
                left,
                right,
            },
            ty,
            span,
        )
    }

    fn lower_string_binary(
        &mut self,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let Some(left) = self.lower_string_view(left, span)? else {
            return Ok(LoweredValue::Diverged);
        };
        // RHS 的控制流会重绑定 owner；保留槽位而非跨 edge 复用旧 ValueId。
        let pending_start = self.pending_operands.len();
        self.pending_operands.push(left);
        let right = self.lower_string_view(right, span)?;
        let left = self.pending_operands.get(pending_start).copied();
        self.pending_operands.truncate(pending_start);
        let Some(right) = right else {
            return Ok(LoweredValue::Diverged);
        };
        let left = left.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let ty = self.expression_ssa_type(expression, span)?;
        let operation = match operator {
            BinaryOperator::Add => Operation::StringConcat { left, right },
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                Operation::StringEqual { left, right }
            }
            _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
        };
        let result = self.append_scalar(operation, ty, span)?;
        self.emit_drops(UnitDropPoint::AfterBinaryOperands(UnitExpressionId::new(
            self.source_unit,
            expression,
        )))?;
        if operator != BinaryOperator::NotEqual {
            return Ok(result);
        }
        let LoweredValue::Value(result) = result else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        self.append_scalar(Operation::BooleanNot { operand: result }, ty, span)
    }

    fn lower_string_view(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<crate::ssa::model::EntityId>, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        // 常量 Name/Member 是逐 use temporary，不从声明的 variable binding 读取。
        if self
            .constant_owned
            .and_then(|owned| {
                owned.materialization_at(UnitExpressionId::new(self.source_unit, expression))
            })
            .is_some()
        {
            return self.lower_string_temporary_view(expression, span);
        }
        match node.payload() {
            Expression::Group { expression } => self.lower_string_view(*expression, span),
            Expression::Name => {
                let symbol = self
                    .references
                    .get(&super::span_key(node.span()))
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, node.span()))?;
                match self.bindings.get(symbol).copied() {
                    Some(LoweredValue::Value(value)) => {
                        Ok(Some(crate::ssa::model::EntityId::Value(value)))
                    }
                    Some(LoweredValue::Unit | LoweredValue::Diverged) | None => Err(
                        lowering_error(LoweringErrorKind::UnsupportedNode, node.span()),
                    ),
                }
            }
            _ => self.lower_string_temporary_view(expression, span),
        }
    }

    fn lower_string_temporary_view(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<crate::ssa::model::EntityId>, LoweringError> {
        match self.lower(expression)? {
            LoweredValue::Value(value) => Ok(Some(crate::ssa::model::EntityId::Value(value))),
            LoweredValue::Diverged => Ok(None),
            LoweredValue::Unit => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
    }

    pub(super) fn checked(
        &mut self,
        operator: CheckedArithmeticOperator,
        left: ValueId,
        right: ValueId,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let boolean = self
            .type_ids
            .iter()
            .find_map(|(frontend, ssa)| {
                (builtin_type(self.typed, *frontend) == Some(BuiltinType::Boolean)).then_some(*ssa)
            })
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::CheckedArithmetic {
                    operator,
                    left,
                    right,
                },
                vec![EntityType::Value(ty), EntityType::Value(boolean)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let baseline = self.bindings.clone();
        let baseline_borrows = self.borrow_bindings.clone();
        let mut carried = self.move_only_carried_bindings(&baseline, span)?;
        let mut carried_loans = self.carried_loans(&baseline_borrows, span)?;
        self.carry_pending_operands(&mut carried, &mut carried_loans, span)?;
        let failure = self.add_carried_control_block(&carried, &carried_loans, span)?;
        let success = self.add_carried_control_block(&carried, &carried_loans, span)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition: require_value(results[1], span)?,
                    when_true: super::cfg::carried_control_edge(failure, &carried, &carried_loans),
                    when_false: super::cfg::carried_control_edge(success, &carried, &carried_loans),
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.function
            .set_terminator(failure, TerminatorKind::Abort, Origin::Source(span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.block = success;
        self.bindings =
            self.rebind_carried_control(&baseline, success, &carried, &carried_loans, span)?;
        self.borrow_bindings =
            self.rebind_carried_loans(success, carried.len(), &carried_loans, span)?;
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }

    fn lower_required_value(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        match self.lower(expression)? {
            LoweredValue::Value(value) => Ok(value),
            LoweredValue::Unit | LoweredValue::Diverged => {
                Err(lowering_error(LoweringErrorKind::MissingFact, span))
            }
        }
    }

    pub(super) fn expression_builtin_type(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<BuiltinType>, LoweringError> {
        let ty = self
            .typed
            .expression_type(UnitExpressionId::new(self.source_unit, expression))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let concrete =
            resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span)?;
        Ok(builtin_type(self.typed, concrete))
    }

    pub(super) fn materialize_unit_value(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        if self.expression_builtin_type(expression, span)? != Some(BuiltinType::Unit) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let ty = self.expression_ssa_type(expression, span)?;
        match self.append_scalar(Operation::Constant(ScalarConstant::Unit), ty, span)? {
            LoweredValue::Value(value) => Ok(value),
            LoweredValue::Unit | LoweredValue::Diverged => {
                Err(lowering_error(LoweringErrorKind::InvalidModel, span))
            }
        }
    }

    pub(super) fn append_scalar(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                operation,
                vec![EntityType::Value(ty)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }
}

pub(super) const fn is_integer_builtin(builtin: BuiltinType) -> bool {
    matches!(
        builtin,
        BuiltinType::Byte
            | BuiltinType::Short
            | BuiltinType::Int
            | BuiltinType::Long
            | BuiltinType::UByte
            | BuiltinType::UShort
            | BuiltinType::UInt
            | BuiltinType::ULong
    )
}

fn checked_operator(operator: BinaryOperator) -> Option<CheckedArithmeticOperator> {
    match operator {
        BinaryOperator::Add => Some(CheckedArithmeticOperator::Add),
        BinaryOperator::Subtract => Some(CheckedArithmeticOperator::Subtract),
        BinaryOperator::Multiply => Some(CheckedArithmeticOperator::Multiply),
        BinaryOperator::Divide => Some(CheckedArithmeticOperator::Divide),
        BinaryOperator::Remainder => Some(CheckedArithmeticOperator::Remainder),
        _ => None,
    }
}

fn comparison_operator(operator: BinaryOperator) -> Option<ComparisonOperator> {
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
