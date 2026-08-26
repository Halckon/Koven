//! SPEC-0197 compilation-unit 基础运算表达式类型检查。

use crate::{
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::SourceUnitId,
    parser::{BinaryOperator, Expression, IntegerLiteralKind, LiteralKind, PrefixOperator},
    source::Span,
    type_checking::{BuiltinType, CompilationUnitTypeError, TypeCheckingError, UnitTypeId},
};

use super::{BodyChecker, ExpressionCheck, flow::extend_facts};

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_prefix(
        &mut self,
        source: SourceUnitId,
        operator: PrefixOperator,
        operator_span: Span,
        operand: crate::ast::ExpressionId,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if operator == PrefixOperator::Minus {
            let operand_node = self
                .file(source)
                .ast()
                .expressions()
                .get(operand)
                .map_err(TypeCheckingError::from)?;
            if let Expression::Literal(LiteralKind::Integer(kind)) = operand_node.payload() {
                let operand_span = operand_node.span();
                if matches!(
                    kind,
                    IntegerLiteralKind::Unsigned | IntegerLiteralKind::UnsignedLong
                ) {
                    let ty = self.literal_type(
                        operand_span,
                        LiteralKind::Integer(*kind),
                        expected,
                        expected_span,
                        false,
                    )?;
                    self.record_expression(source, operand, ty);
                    self.emit_operand_error(operator_span, operand_span)?;
                    return Ok(ExpressionCheck {
                        ty: self.error_type(),
                        falls_through: true,
                    });
                }
                let ty = self.literal_type(
                    operand_span,
                    LiteralKind::Integer(*kind),
                    expected,
                    expected_span,
                    true,
                )?;
                self.record_expression(source, operand, ty);
                return Ok(ExpressionCheck {
                    ty,
                    falls_through: true,
                });
            }
        }
        let operand_result = self.check_expression(source, operand, None, None, return_type)?;
        let valid = match operator {
            PrefixOperator::Not => self.is_builtin(operand_result.ty, BuiltinType::Boolean),
            PrefixOperator::Plus | PrefixOperator::Minus => self.is_numeric(operand_result.ty),
        };
        if !valid && !self.is_error(operand_result.ty) {
            let operand_span = self
                .file(source)
                .ast()
                .expressions()
                .get(operand)
                .map_err(TypeCheckingError::from)?
                .span();
            self.emit_operand_error(operator_span, operand_span)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: operand_result.falls_through,
            });
        }
        Ok(operand_result)
    }

    pub(super) fn check_binary(
        &mut self,
        source: SourceUnitId,
        left: crate::ast::ExpressionId,
        operator: BinaryOperator,
        operator_span: Span,
        right: crate::ast::ExpressionId,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if matches!(
            operator,
            BinaryOperator::InclusiveRange
                | BinaryOperator::ExclusiveRange
                | BinaryOperator::To
                | BinaryOperator::Elvis
                | BinaryOperator::In
                | BinaryOperator::NotIn
        ) {
            return Err(CompilationUnitTypeError::UnsupportedBody(operator_span));
        }
        if matches!(
            operator,
            BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr
        ) {
            let left_result = self.check_expression(source, left, None, None, return_type)?;
            let (left_true, left_false) = self.condition_facts(source, left)?;
            let baseline = self.flow_facts.clone();
            let right_entry = if operator == BinaryOperator::LogicalAnd {
                &left_true
            } else {
                &left_false
            };
            self.flow_facts = extend_facts(&baseline, right_entry);
            let right_result = self.check_expression(source, right, None, None, return_type);
            self.flow_facts = baseline;
            let right_result = right_result?;
            let boolean = self.builtin(BuiltinType::Boolean);
            let ty = if self.is_builtin(left_result.ty, BuiltinType::Boolean)
                && self.is_builtin(right_result.ty, BuiltinType::Boolean)
            {
                boolean
            } else if self.is_error(left_result.ty) || self.is_error(right_result.ty) {
                self.error_type()
            } else {
                self.emit_binary_operand_error(
                    source,
                    operator_span,
                    left,
                    left_result.ty,
                    right,
                    right_result.ty,
                )?;
                self.error_type()
            };
            return Ok(ExpressionCheck {
                ty,
                falls_through: left_result.falls_through && right_result.falls_through,
            });
        }
        let left_result = self.check_expression(source, left, None, None, return_type)?;
        let right_result = self.check_expression(source, right, None, None, return_type)?;
        let boolean = self.builtin(BuiltinType::Boolean);
        let result = match operator {
            BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Remainder
            | BinaryOperator::Subtract => (left_result.ty == right_result.ty
                && self.is_numeric(left_result.ty))
            .then_some(left_result.ty),
            BinaryOperator::Add => (left_result.ty == right_result.ty
                && (self.is_numeric(left_result.ty)
                    || self.is_builtin(left_result.ty, BuiltinType::String)))
            .then_some(left_result.ty),
            BinaryOperator::Less
            | BinaryOperator::Greater
            | BinaryOperator::LessEqual
            | BinaryOperator::GreaterEqual => (left_result.ty == right_result.ty
                && (self.is_numeric(left_result.ty)
                    || self.is_builtin(left_result.ty, BuiltinType::Char)))
            .then_some(boolean),
            BinaryOperator::Equal | BinaryOperator::NotEqual => (self
                .assignable(left_result.ty, right_result.ty)
                || self.assignable(right_result.ty, left_result.ty))
            .then_some(boolean),
            BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr => (self
                .is_builtin(left_result.ty, BuiltinType::Boolean)
                && self.is_builtin(right_result.ty, BuiltinType::Boolean))
            .then_some(boolean),
            BinaryOperator::InclusiveRange
            | BinaryOperator::ExclusiveRange
            | BinaryOperator::To
            | BinaryOperator::Elvis
            | BinaryOperator::In
            | BinaryOperator::NotIn => unreachable!("deferred operators are rejected above"),
        };
        let ty = if let Some(result) = result {
            result
        } else if self.is_error(left_result.ty) || self.is_error(right_result.ty) {
            self.error_type()
        } else {
            self.emit_binary_operand_error(
                source,
                operator_span,
                left,
                left_result.ty,
                right,
                right_result.ty,
            )?;
            self.error_type()
        };
        Ok(ExpressionCheck {
            ty,
            falls_through: left_result.falls_through && right_result.falls_through,
        })
    }

    fn emit_operand_error(
        &mut self,
        operator_span: Span,
        operand_span: Span,
    ) -> Result<(), CompilationUnitTypeError> {
        self.emit_with_label_from_operator(
            codes::INVALID_OPERAND_TYPES,
            "operator does not accept this operand type",
            operator_span,
            operand_span,
            "invalid operand",
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_binary_operand_error(
        &mut self,
        source: SourceUnitId,
        operator_span: Span,
        left: crate::ast::ExpressionId,
        left_ty: UnitTypeId,
        right: crate::ast::ExpressionId,
        right_ty: UnitTypeId,
    ) -> Result<(), CompilationUnitTypeError> {
        let code = codes::catalog()?.resolve(codes::INVALID_OPERAND_TYPES)?;
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            "operator does not accept these operand types",
            operator_span,
        )?;
        diagnostic.add_label(
            self.sources,
            self.file(source)
                .ast()
                .expressions()
                .get(left)
                .map_err(TypeCheckingError::from)?
                .span(),
            format!("left operand has type {}", self.type_name(left_ty)),
        )?;
        diagnostic.add_label(
            self.sources,
            self.file(source)
                .ast()
                .expressions()
                .get(right)
                .map_err(TypeCheckingError::from)?
                .span(),
            format!("right operand has type {}", self.type_name(right_ty)),
        )?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn is_numeric(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.signatures.types().get(ty),
            Some(crate::type_checking::UnitTypeKind::Builtin(
                BuiltinType::Byte
                    | BuiltinType::Short
                    | BuiltinType::Int
                    | BuiltinType::Long
                    | BuiltinType::UByte
                    | BuiltinType::UShort
                    | BuiltinType::UInt
                    | BuiltinType::ULong
                    | BuiltinType::Float
                    | BuiltinType::Double
            ))
        )
    }

    fn emit_with_label_from_operator(
        &mut self,
        code: &str,
        message: &str,
        primary: Span,
        label: Span,
        label_message: &str,
    ) -> Result<(), CompilationUnitTypeError> {
        let code = codes::catalog()?.resolve(code)?;
        let mut diagnostic =
            Diagnostic::new(self.sources, Severity::Error, code, message, primary)?;
        diagnostic.add_label(self.sources, label, label_message)?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }
}
